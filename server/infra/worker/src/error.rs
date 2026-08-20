use std::{error::Error, fmt, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerClosed;

impl fmt::Display for WorkerClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker is closed and cannot accept new tasks")
    }
}

impl Error for WorkerClosed {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerTimeout {
    timeout: Duration,
}

impl WorkerTimeout {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub fn timeout(self) -> Duration {
        self.timeout
    }
}

impl fmt::Display for WorkerTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "worker operation exceeded its {} millisecond timeout",
            self.timeout.as_millis()
        )
    }
}

impl Error for WorkerTimeout {}
