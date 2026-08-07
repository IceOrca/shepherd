use std::{error::Error, fmt};

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
}

impl QueueConfig {
    pub fn new(capacity: usize, concurrency: usize) -> Result<Self, QueueConfigError> {
        if capacity == 0 {
            return Err(QueueConfigError::ZeroCapacity);
        }
        if concurrency == 0 {
            return Err(QueueConfigError::ZeroConcurrency);
        }
        Ok(Self {
            capacity,
            concurrency,
            shutdown_mode: QueueShutdownMode::Drain,
        })
    }

    pub fn with_shutdown_mode(mut self, shutdown_mode: QueueShutdownMode) -> Self {
        self.shutdown_mode = shutdown_mode;
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
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            capacity: 100,
            concurrency: 1,
            shutdown_mode: QueueShutdownMode::Drain,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueConfigError {
    ZeroCapacity,
    ZeroConcurrency,
}

impl fmt::Display for QueueConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("queue capacity must be greater than zero"),
            Self::ZeroConcurrency => formatter.write_str("queue concurrency must be greater than zero"),
        }
    }
}

impl Error for QueueConfigError {}

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
    use super::{QueueConfig, QueueConfigError};

    #[test]
    fn queue_config_rejects_zero_limits() {
        assert_eq!(QueueConfig::new(0, 1), Err(QueueConfigError::ZeroCapacity));
        assert_eq!(QueueConfig::new(1, 0), Err(QueueConfigError::ZeroConcurrency));
    }
}
