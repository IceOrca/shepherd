use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info_span};

use crate::{AsyncWorker, QueueConfig, TaskSender, WorkerClosed, blocking_queue, queue::bounded, state::WorkerState};

/// Supervises CPU-heavy or synchronous functions through Tokio spawn_blocking.
///
/// Tokio cannot forcibly stop a blocking closure after it starts. Long-running
/// closures should periodically check the provided cancellation token.
#[derive(Clone)]
pub struct BlockingWorker {
    state: WorkerState,
}

impl BlockingWorker {
    pub fn new() -> Self {
        Self::from_state(WorkerState::new())
    }

    pub(crate) fn from_state(state: WorkerState) -> Self {
        Self { state }
    }

    pub fn spawn<F, T>(&self, name: impl Into<String>, task: F) -> Result<JoinHandle<T>, WorkerClosed>
    where
        F: FnOnce(CancellationToken) -> T + Send + 'static,
        T: Send + 'static,
    {
        if self.state.is_closed() {
            return Err(WorkerClosed);
        }

        let name = name.into();
        let cancellation = self.state.child_token();
        let closure = move || {
            let span = info_span!("blocking_worker_task", task.name = %name);
            let _entered = span.enter();
            debug!(task.name = %name, "Blocking worker task started");
            let output = task(cancellation);
            debug!(task.name = %name, "Blocking worker task finished");
            output
        };
        Ok(self.state.tracker().spawn_blocking(closure))
    }

    pub fn start_queue<T, H>(
        &self,
        name: impl Into<String>,
        config: QueueConfig,
        handler: H,
    ) -> Result<TaskSender<T>, WorkerClosed>
    where
        T: Send + 'static,
        H: Fn(T, CancellationToken) + Send + Sync + 'static,
    {
        let name = name.into();
        let (sender, receiver, queue_shutdown) = bounded(config);
        let dispatcher_name = format!("{name}.dispatcher");
        let dispatcher = AsyncWorker::from_state(self.state.clone());
        let handle = dispatcher.spawn(dispatcher_name, move |worker_shutdown| {
            blocking_queue::run(name, config, receiver, queue_shutdown, worker_shutdown, handler)
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

impl Default for BlockingWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::BlockingWorker;
    use crate::QueueConfig;

    #[tokio::test]
    async fn blocking_worker_returns_the_closure_output() {
        let worker = BlockingWorker::new();
        let handle = worker
            .spawn("calculation", |_cancellation| 21 * 2)
            .expect("open worker should accept a task");

        assert_eq!(handle.await.expect("blocking task should join"), 42);
        worker.wait().await;
    }

    #[tokio::test]
    async fn blocking_queue_drains_buffered_tasks() {
        let worker = BlockingWorker::new();
        let total = Arc::new(AtomicUsize::new(0));
        let handler_total = Arc::clone(&total);
        let sender = worker
            .start_queue(
                "numbers",
                QueueConfig::new(4, 2).expect("valid queue configuration"),
                move |value, _cancellation| {
                    handler_total.fetch_add(value, Ordering::SeqCst);
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
    async fn closed_blocking_worker_rejects_new_tasks() {
        let worker = BlockingWorker::new();
        worker.wait().await;

        let result = worker.spawn("too-late", |_cancellation| ());
        assert!(result.is_err());
    }
}
