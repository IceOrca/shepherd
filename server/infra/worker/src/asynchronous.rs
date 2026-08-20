use std::{future::Future, time::Duration};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, info_span, warn};

use crate::{QueueConfig, TaskSender, WorkerClosed, WorkerTimeout, async_queue, queue::bounded, state::WorkerState};

/// Supervises futures spawned on Tokio's asynchronous executor.
#[derive(Clone)]
pub struct AsyncWorker {
    state: WorkerState,
}

impl AsyncWorker {
    pub fn new() -> Self {
        Self::from_state(WorkerState::new())
    }

    pub(crate) fn from_state(state: WorkerState) -> Self {
        Self { state }
    }

    pub fn spawn<F, Fut, T>(&self, name: impl Into<String>, task: F) -> Result<JoinHandle<T>, WorkerClosed>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        if self.state.is_closed() {
            return Err(WorkerClosed);
        }

        let name = name.into();
        let cancellation = self.state.child_token();
        let future = async move {
            let span = info_span!("async_worker_task", task.name = %name);
            async move {
                debug!(task.name = %name, "Async worker task started");
                let output = task(cancellation).await;
                debug!(task.name = %name, "Async worker task finished");
                output
            }
            .instrument(span)
            .await
        };
        Ok(self.state.tracker().spawn(future))
    }

    /// Spawns finite asynchronous work with an execution deadline.
    ///
    /// Long-lived service loops should use [`Self::spawn`] and cooperate with
    /// the supplied cancellation token instead.
    pub fn spawn_with_timeout<F, Fut, T>(
        &self,
        name: impl Into<String>,
        timeout: Duration,
        task: F,
    ) -> Result<JoinHandle<Result<T, WorkerTimeout>>, WorkerClosed>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let name: String = name.into();
        let timed_task_name: String = name.clone();
        self.spawn(name, move |cancellation: CancellationToken| async move {
            let task_cancellation: CancellationToken = cancellation.clone();
            let result: Result<T, tokio::time::error::Elapsed> =
                tokio::time::timeout(timeout, task(task_cancellation)).await;
            match result {
                Ok(output) => Ok(output),
                Err(_elapsed) => {
                    cancellation.cancel();
                    warn!(
                        task.name = %timed_task_name,
                        timeout_ms = timeout.as_millis(),
                        "Finite async worker task timed out"
                    );
                    Err(WorkerTimeout::new(timeout))
                }
            }
        })
    }

    pub fn start_queue<T, H, Fut>(
        &self,
        name: impl Into<String>,
        config: QueueConfig,
        handler: H,
    ) -> Result<TaskSender<T>, WorkerClosed>
    where
        T: Send + 'static,
        H: Fn(T, CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let name = name.into();
        let (sender, receiver, queue_shutdown) = bounded(config);
        let dispatcher_name = format!("{name}.dispatcher");
        let handle = self.spawn(dispatcher_name, move |worker_shutdown| {
            async_queue::run(name, config, receiver, queue_shutdown, worker_shutdown, handler)
        })?;
        drop(handle);
        Ok(sender)
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.state.child_token()
    }

    pub fn cancel(&self) {
        self.state.cancel();
    }

    pub async fn shutdown(&self) {
        self.state.shutdown().await;
    }

    pub async fn shutdown_with_timeout(&self, timeout: Duration) -> Result<(), WorkerTimeout> {
        self.state.shutdown_with_timeout(timeout).await
    }

    pub async fn wait(&self) {
        self.state.wait().await;
    }
}

impl Default for AsyncWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::AsyncWorker;
    use crate::{QueueConfig, QueueShutdownMode, WorkerTimeout};

    #[tokio::test]
    async fn shutdown_cancels_and_waits_for_async_tasks() {
        let worker = AsyncWorker::new();
        let stopped = Arc::new(AtomicBool::new(false));
        let task_stopped = Arc::clone(&stopped);

        worker
            .spawn("cancellable", move |cancellation| async move {
                cancellation.cancelled().await;
                task_stopped.store(true, Ordering::SeqCst);
            })
            .expect("open worker should accept a task");

        worker.shutdown().await;
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn async_queue_drains_buffered_tasks() {
        use std::sync::atomic::AtomicUsize;

        let worker = AsyncWorker::new();
        let total = Arc::new(AtomicUsize::new(0));
        let handler_total = Arc::clone(&total);
        let sender = worker
            .start_queue(
                "numbers",
                QueueConfig::new(4, 2).expect("valid queue configuration"),
                move |value, _cancellation| {
                    let task_total = Arc::clone(&handler_total);
                    async move {
                        task_total.fetch_add(value, Ordering::SeqCst);
                    }
                },
            )
            .expect("open worker should start a queue");

        sender.send(2).await.expect("queue should accept first task");
        sender.send(3).await.expect("queue should accept second task");
        sender.shutdown();
        worker.wait().await;

        assert_eq!(total.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn immediate_queue_shutdown_cancels_active_and_discards_buffered_tasks() {
        use std::{sync::atomic::AtomicUsize, time::Duration};

        use tokio::sync::Notify;

        let worker = AsyncWorker::new();
        let started = Arc::new(Notify::new());
        let handler_started = Arc::clone(&started);
        let completed = Arc::new(AtomicUsize::new(0));
        let handler_completed = Arc::clone(&completed);
        let sender = worker
            .start_queue(
                "immediate",
                QueueConfig::new(4, 1)
                    .expect("valid queue configuration")
                    .with_shutdown_mode(QueueShutdownMode::Immediate),
                move |value, cancellation| {
                    let task_started = Arc::clone(&handler_started);
                    let task_completed = Arc::clone(&handler_completed);
                    async move {
                        if value == 1 {
                            task_started.notify_one();
                            cancellation.cancelled().await;
                        }
                        task_completed.fetch_add(value, Ordering::SeqCst);
                    }
                },
            )
            .expect("open worker should start a queue");

        sender.send(1).await.expect("queue should accept active task");
        started.notified().await;
        sender.send(10).await.expect("queue should accept buffered task");
        sender.shutdown();
        tokio::time::timeout(Duration::from_secs(1), worker.wait())
            .await
            .expect("immediate shutdown should not wait for buffered work");

        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn closed_async_worker_rejects_new_tasks() {
        let worker = AsyncWorker::new();
        worker.wait().await;

        let result = worker.spawn("too-late", |_cancellation| async {});
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn finite_async_task_returns_timeout_error() {
        use std::time::Duration;

        let worker: AsyncWorker = AsyncWorker::new();
        let handle: tokio::task::JoinHandle<Result<(), WorkerTimeout>> = worker
            .spawn_with_timeout("never-finishes", Duration::from_millis(10), |_cancellation| async {
                std::future::pending::<()>().await;
            })
            .expect("open worker should accept a task");

        let result: Result<(), WorkerTimeout> = handle.await.expect("timed task should join");
        assert_eq!(result, Err(WorkerTimeout::new(Duration::from_millis(10))));
        worker.wait().await;
    }

    #[tokio::test]
    async fn async_queue_timeout_releases_concurrency_for_later_work() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            },
            time::Duration,
        };

        let worker: AsyncWorker = AsyncWorker::new();
        let completed_count: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let handler_completed_count: Arc<AtomicUsize> = Arc::clone(&completed_count);
        let queue_config: QueueConfig = QueueConfig::new(2, 1)
            .expect("valid queue configuration")
            .with_task_timeout(Duration::from_millis(10));
        let sender: crate::TaskSender<u32> = worker
            .start_queue(
                "timeout-queue",
                queue_config,
                move |value: u32, _cancellation: tokio_util::sync::CancellationToken| {
                    let task_completed_count: Arc<AtomicUsize> = Arc::clone(&handler_completed_count);
                    async move {
                        if value == 1 {
                            std::future::pending::<()>().await;
                        }
                        task_completed_count.fetch_add(1, Ordering::SeqCst);
                    }
                },
            )
            .expect("open worker should start a queue");

        sender.send(1).await.expect("queue should accept timed task");
        sender.send(2).await.expect("queue should accept later task");
        sender.shutdown();
        worker.wait().await;

        assert_eq!(completed_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn shutdown_timeout_bounds_non_cooperative_async_task() {
        use std::time::Duration;

        let worker: AsyncWorker = AsyncWorker::new();
        let task_handle: tokio::task::JoinHandle<()> = worker
            .spawn("non-cooperative", |_cancellation| async {
                std::future::pending::<()>().await;
            })
            .expect("open worker should accept a task");
        drop(task_handle);

        let result: Result<(), WorkerTimeout> = worker.shutdown_with_timeout(Duration::from_millis(10)).await;
        assert_eq!(result, Err(WorkerTimeout::new(Duration::from_millis(10))));
    }
}
