#![cfg_attr(debug_assertions, allow(unused))]

mod async_queue;
mod asynchronous;
mod blocking;
mod blocking_queue;
mod error;
mod queue;
mod state;

pub use asynchronous::AsyncWorker;
pub use blocking::BlockingWorker;
pub use error::WorkerClosed;
pub use queue::{QueueConfig, QueueConfigError, QueueShutdownMode, TaskSender};

use tokio_util::sync::CancellationToken;

use state::WorkerState;

/// Shared lifecycle for asynchronous and blocking in-process tasks.
#[derive(Clone)]
pub struct Worker {
    asynchronous: AsyncWorker,
    blocking: BlockingWorker,
    state: WorkerState,
}

impl Worker {
    pub fn new() -> Self {
        let state = WorkerState::new();
        Self {
            asynchronous: AsyncWorker::from_state(state.clone()),
            blocking: BlockingWorker::from_state(state.clone()),
            state,
        }
    }

    pub fn asynchronous(&self) -> &AsyncWorker {
        &self.asynchronous
    }

    pub fn blocking(&self) -> &BlockingWorker {
        &self.blocking
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

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}
