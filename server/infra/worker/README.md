# Infra Worker

infra-worker supervises in-process background work without hiding Tokio. Use AsyncWorker for futures and
BlockingWorker for CPU-heavy or synchronous closures. Worker combines both with shared cancellation and graceful
shutdown.

Tasks can be spawned directly or submitted through a bounded in-memory channel. The channel provides backpressure and
a concurrency limit, but it is not durable: buffered tasks are lost when the process terminates. Keep infra-jobs
for a future persistent queue.

Direct task:

    let handle = worker.asynchronous().spawn("listener", |cancel| async move {
        cancel.cancelled().await;
    })?;

Bounded async queue:

    let sender = worker.asynchronous().start_queue(
        "notifications",
        QueueConfig::new(100, 4)?,
        |notification, cancel| async move {
            send_notification(notification, cancel).await;
        },
    )?;

    sender.send(notification).await?;
    sender.shutdown();
    worker.wait().await;

QueueConfig defaults to capacity 100, concurrency 1, and Drain shutdown. Immediate shutdown discards buffered work and
cooperatively cancels active handlers. Tokio cannot forcibly stop blocking closures, so blocking handlers must inspect
their cancellation token when early termination matters.
