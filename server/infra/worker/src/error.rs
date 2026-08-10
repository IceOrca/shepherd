use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerClosed;

impl fmt::Display for WorkerClosed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("worker is closed and cannot accept new tasks")
    }
}

impl Error for WorkerClosed {}
