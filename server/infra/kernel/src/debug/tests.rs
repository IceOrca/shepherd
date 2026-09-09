use std::sync::Arc;

use tokio::sync::{RwLock, RwLockWriteGuard, oneshot};
use tracing_subscriber::{EnvFilter, Registry, layer::Layered, prelude::*, reload};

use super::LogMutationCtx;

type TestSubscriber = Layered<reload::Layer<EnvFilter, Registry>, Registry>;

fn fixture() -> (TestSubscriber, Arc<RwLock<LogMutationCtx>>) {
    let (layer, handle) = reload::Layer::new(EnvFilter::new("info"));
    (
        tracing_subscriber::registry().with(layer),
        Arc::new(RwLock::new(LogMutationCtx {
            handle,
            filter_text: "info".to_owned(),
        })),
    )
}

#[tokio::test]
async fn concurrent_log_changes_keep_before_and_after_in_one_guard() {
    let (_subscriber, context) = fixture();
    let change = async |level: &str| -> (String, String) {
        let mut mutation: RwLockWriteGuard<'_, LogMutationCtx> = context.write().await;
        let before: String = mutation.filter().to_owned();
        tokio::task::yield_now().await; // Simulate the asynchronous audit write.
        let after: String = mutation.set_level(level).expect("live reload");
        (before, after)
    };
    let (first, second) = tokio::join!(change("debug"), change("trace"));
    let final_filter: String = context.read().await.filter().to_owned();
    if first.0 == "info" {
        assert_eq!(second.0, first.1);
        assert_eq!(final_filter, second.1);
    } else {
        assert_eq!(first.0, second.1);
        assert_eq!(second.0, "info");
        assert_eq!(final_filter, first.1);
    }
}

#[tokio::test]
async fn failed_reload_keeps_the_previous_projection() {
    let (subscriber, context) = fixture();
    let mut mutation: RwLockWriteGuard<'_, LogMutationCtx> = context.write().await;
    assert!(mutation.set_level("wire=trace").is_err());
    assert_eq!(mutation.filter(), "info");
    drop(subscriber);
    assert!(mutation.set_level("debug").is_err());
    assert_eq!(mutation.filter(), "info");
}

#[tokio::test]
async fn cancelling_a_log_mutation_releases_its_context() {
    let (_subscriber, context) = fixture();
    let (entered, wait): (oneshot::Sender<()>, oneshot::Receiver<()>) = oneshot::channel();
    let writer: Arc<RwLock<LogMutationCtx>> = Arc::clone(&context);
    let task: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        let _mutation: RwLockWriteGuard<'_, LogMutationCtx> = writer.write().await;
        entered.send(()).expect("waiting test");
        std::future::pending::<()>().await;
    });
    wait.await.expect("mutation acquired");
    assert!(context.try_read().is_err());
    task.abort();
    assert!(task.await.expect_err("cancelled mutation").is_cancelled());
    let mut mutation: RwLockWriteGuard<'_, LogMutationCtx> = context.try_write().expect("released guard");
    assert_eq!(mutation.filter(), "info");
    assert!(mutation.set_level("debug").is_ok());
}
