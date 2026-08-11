#![cfg_attr(debug_assertions, allow(unused))]

#[cfg(any(feature = "multi-threaded", feature = "single-threaded"))]
mod ntree;
#[cfg(feature = "tokio-lock")]
mod ntree_async;

#[cfg(feature = "tokio-lock")]
pub use ntree_async::{AsyncEntry, AsyncIter, AsyncNTree};
#[cfg(any(feature = "multi-threaded", feature = "single-threaded"))]
pub use ntree::{Entry, Iter, NTree};
