use std::{future::Future, sync::Arc};

use tokio::sync::{Semaphore, mpsc};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{Instrument, debug, info_span, warn};

use crate::{QueueConfig, QueueShutdownMode};

pub(crate) async fn run<T, H, Fut>(
    name: String,
    config: QueueConfig,
    mut receiver: mpsc::Receiver<T>,
    queue_shutdown: CancellationToken,
    worker_shutdown: CancellationToken,
    handler: H,
) where
    T: Send + 'static,
    H: Fn(T, CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let handler = Arc::new(handler);
    let semaphore = Arc::new(Semaphore::new(config.concurrency()));
    let active_tasks = TaskTracker::new();
    let task_cancellation = CancellationToken::new();
    let mut draining = false;

    loop {
        let task = if draining {
            receiver.recv().await
        } else {
            tokio::select! {
                _ = worker_shutdown.cancelled() => {
                    begin_shutdown(
                        &mut receiver,
                        config.shutdown_mode(),
                        &task_cancellation,
                        &mut draining,
                    );
                    if !draining {
                        break;
                    }
                    continue;
                }
                _ = queue_shutdown.cancelled() => {
                    begin_shutdown(
                        &mut receiver,
                        config.shutdown_mode(),
                        &task_cancellation,
                        &mut draining,
                    );
                    if !draining {
                        break;
                    }
                    continue;
                }
                task = receiver.recv() => task,
            }
        };

        let Some(task) = task else {
            break;
        };
        let permit = if draining {
            Arc::clone(&semaphore).acquire_owned().await
        } else {
            tokio::select! {
                permit = Arc::clone(&semaphore).acquire_owned() => permit,
                _ = worker_shutdown.cancelled() => {
                    begin_shutdown(&mut receiver, config.shutdown_mode(), &task_cancellation, &mut draining);
                    if !draining {
                        break;
                    }
                    Arc::clone(&semaphore).acquire_owned().await
                }
                _ = queue_shutdown.cancelled() => {
                    begin_shutdown(&mut receiver, config.shutdown_mode(), &task_cancellation, &mut draining);
                    if !draining {
                        break;
                    }
                    Arc::clone(&semaphore).acquire_owned().await
                }
            }
        };
        let permit = match permit {
            Ok(permit) => permit,
            Err(_closed) => break,
        };

        if !draining && (worker_shutdown.is_cancelled() || queue_shutdown.is_cancelled()) {
            begin_shutdown(&mut receiver, config.shutdown_mode(), &task_cancellation, &mut draining);
            if !draining {
                drop(permit);
                break;
            }
        }

        let handler = Arc::clone(&handler);
        let cancellation = task_cancellation.child_token();
        let task_name = name.clone();
        active_tasks.spawn(async move {
            let _permit = permit;
            let span = info_span!("async_queue_task", queue.name = %task_name);
            async move {
                debug!(queue.name = %task_name, "Async queued task started");
                let task_cancellation: CancellationToken = cancellation.clone();
                if let Some(task_timeout) = config.task_timeout() {
                    let result: Result<(), tokio::time::error::Elapsed> =
                        tokio::time::timeout(task_timeout, handler(task, task_cancellation)).await;
                    match result {
                        Ok(()) => debug!(queue.name = %task_name, "Async queued task finished"),
                        Err(_elapsed) => {
                            cancellation.cancel();
                            warn!(
                                queue.name = %task_name,
                                timeout_ms = task_timeout.as_millis(),
                                "Async queued task timed out"
                            );
                        }
                    }
                } else {
                    handler(task, task_cancellation).await;
                    debug!(queue.name = %task_name, "Async queued task finished");
                }
            }
            .instrument(span)
            .await;
        });
    }

    active_tasks.close();
    active_tasks.wait().await;
}

fn begin_shutdown<T>(
    receiver: &mut mpsc::Receiver<T>,
    shutdown_mode: QueueShutdownMode,
    task_cancellation: &CancellationToken,
    draining: &mut bool,
) {
    receiver.close();
    match shutdown_mode {
        QueueShutdownMode::Drain => *draining = true,
        QueueShutdownMode::Immediate => {
            task_cancellation.cancel();
            *draining = false;
        }
    }
}
