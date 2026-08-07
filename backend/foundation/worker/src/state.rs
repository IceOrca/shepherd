use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Clone)]
pub(crate) struct WorkerState {
    cancellation: CancellationToken,
    tracker: TaskTracker,
}

impl WorkerState {
    pub(crate) fn new() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            tracker: TaskTracker::new(),
        }
    }

    pub(crate) fn child_token(&self) -> CancellationToken {
        self.cancellation.child_token()
    }

    pub(crate) fn tracker(&self) -> &TaskTracker {
        &self.tracker
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.tracker.is_closed()
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(crate) async fn shutdown(&self) {
        self.cancel();
        self.wait().await;
    }

    pub(crate) async fn wait(&self) {
        self.tracker.close();
        self.tracker.wait().await;
    }
}
