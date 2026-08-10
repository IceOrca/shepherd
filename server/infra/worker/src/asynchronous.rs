use std::future::Future;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, info_span};

use crate::{QueueConfig, TaskSender, WorkerClosed, async_queue, queue::bounded, state::WorkerState};

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
    use crate::{QueueConfig, QueueShutdownMode};

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
}
