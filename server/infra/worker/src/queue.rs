use std::{error::Error, fmt, time::Duration};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueShutdownMode {
    /// Stop accepting messages and finish queued and active work.
    Drain,
    /// Discard queued work and cooperatively cancel active work.
    Immediate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueConfig {
    capacity: usize,
    concurrency: usize,
    shutdown_mode: QueueShutdownMode,
    task_timeout: Option<Duration>,
}

impl QueueConfig {
    pub fn new(capacity: usize, concurrency: usize) -> Result<Self, QueueCfgErr> {
        if capacity == 0 {
            return Err(QueueCfgErr::ZeroCapacity);
        }
        if concurrency == 0 {
            return Err(QueueCfgErr::ZeroConcurrency);
        }
        Ok(Self {
            capacity,
            concurrency,
            shutdown_mode: QueueShutdownMode::Drain,
            task_timeout: None,
        })
    }

    pub fn with_shutdown_mode(mut self, shutdown_mode: QueueShutdownMode) -> Self {
        self.shutdown_mode = shutdown_mode;
        self
    }

    pub fn with_task_timeout(mut self, task_timeout: Duration) -> Self {
        self.task_timeout = Some(task_timeout);
        self
    }

    pub fn capacity(self) -> usize {
        self.capacity
    }

    pub fn concurrency(self) -> usize {
        self.concurrency
    }

    pub fn shutdown_mode(self) -> QueueShutdownMode {
        self.shutdown_mode
    }

    pub fn task_timeout(self) -> Option<Duration> {
        self.task_timeout
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            capacity: 100,
            concurrency: 1,
            shutdown_mode: QueueShutdownMode::Drain,
            task_timeout: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueCfgErr {
    ZeroCapacity,
    ZeroConcurrency,
}

impl fmt::Display for QueueCfgErr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("queue capacity must be greater than zero"),
            Self::ZeroConcurrency => formatter.write_str("queue concurrency must be greater than zero"),
        }
    }
}

impl Error for QueueCfgErr {}

pub struct TaskSender<T> {
    sender: mpsc::Sender<T>,
    shutdown: CancellationToken,
}

impl<T> TaskSender<T> {
    pub async fn send(&self, task: T) -> Result<(), mpsc::error::SendError<T>> {
        if self.shutdown.is_cancelled() {
            return Err(mpsc::error::SendError(task));
        }
        self.sender.send(task).await
    }

    pub fn try_send(&self, task: T) -> Result<(), mpsc::error::TrySendError<T>> {
        if self.shutdown.is_cancelled() {
            return Err(mpsc::error::TrySendError::Closed(task));
        }
        self.sender.try_send(task)
    }

    /// Stops this queue. Its configured shutdown mode controls whether buffered
    /// work is drained or discarded.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    pub fn is_closed(&self) -> bool {
        self.shutdown.is_cancelled() || self.sender.is_closed()
    }

    pub fn capacity(&self) -> usize {
        self.sender.capacity()
    }
}

impl<T> Clone for TaskSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            shutdown: self.shutdown.clone(),
        }
    }
}

pub(crate) fn bounded<T>(config: QueueConfig) -> (TaskSender<T>, mpsc::Receiver<T>, CancellationToken) {
    let (sender, receiver) = mpsc::channel(config.capacity());
    let shutdown = CancellationToken::new();
    (
        TaskSender {
            sender,
            shutdown: shutdown.clone(),
        },
        receiver,
        shutdown,
    )
}

#[cfg(test)]
mod tests {
    use super::{QueueConfig, QueueCfgErr};

    #[test]
    fn queue_config_rejects_zero_limits() {
        assert_eq!(QueueConfig::new(0, 1), Err(QueueCfgErr::ZeroCapacity));
        assert_eq!(QueueConfig::new(1, 0), Err(QueueCfgErr::ZeroConcurrency));
    }
}
