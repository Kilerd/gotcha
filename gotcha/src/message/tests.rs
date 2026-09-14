use super::*;
use crate::{ConfigWrapper, EmptyConfig};
use futures_util::{task::noop_waker_ref, FutureExt};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::task::Context;
use std::time::Duration;
use tokio::sync::Notify;

#[derive(Clone, Default)]
struct AppState {
    greeting: String,
}

struct Greet {
    name: String,
}

#[async_trait]
impl Message<AppState, EmptyConfig> for Greet {
    type Output = String;
    async fn handle(self, messager: Messager<AppState, EmptyConfig>) -> String {
        format!("{}, {}!", messager.state().greeting, self.name)
    }
}

#[test]
fn send_dispatches_and_reads_state() {
    let context = GotchaContext {
        config: ConfigWrapper {
            server: Default::default(),
            app: EmptyConfig::default(),
        },
        state: AppState { greeting: "Hello".to_string() },
    };
    let messager = Messager::new(context);

    let output = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(messager.send(Greet { name: "world".to_string() }));

    assert_eq!(output, "Hello, world!");
}

fn messager() -> Messager<(), ()> {
    Messager::new(GotchaContext {
        config: ConfigWrapper::default(),
        state: (),
    })
}

struct Outcome(Result<u32, &'static str>);

#[async_trait]
impl Message<(), ()> for Outcome {
    type Output = Result<u32, &'static str>;

    async fn handle(self, _: Messager<(), ()>) -> Self::Output {
        self.0
    }
}

struct Panics;

#[async_trait]
impl Message<(), ()> for Panics {
    type Output = ();

    async fn handle(self, _: Messager<(), ()>) {
        panic!("message failed");
    }
}

#[tokio::test]
async fn results_and_panics_reach_the_caller_through_the_correct_boundary() {
    let messager = messager();
    for output in [Ok(42), Err("rejected")] {
        assert_eq!(messager.send(Outcome(output)).await, output);
        assert_eq!(messager.spawn(Outcome(output)).await.unwrap(), output);
    }

    let panic = AssertUnwindSafe(messager.send(Panics)).catch_unwind().await.unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"message failed"));
    let error = messager.spawn(Panics).await.unwrap_err();
    assert!(error.is_panic());
    assert_eq!(error.into_panic().downcast_ref::<&str>(), Some(&"message failed"));
}

#[derive(Clone, Default)]
struct WaitForRelease {
    started: Arc<Notify>,
    release: Arc<Notify>,
    dropped: Arc<Notify>,
}

struct DropSignal(Arc<Notify>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[async_trait]
impl Message<(), ()> for WaitForRelease {
    type Output = u32;

    async fn handle(self, _: Messager<(), ()>) -> u32 {
        let _resource = DropSignal(self.dropped);
        self.started.notify_one();
        self.release.notified().await;
        42
    }
}

#[test]
fn send_is_lazy_and_dropping_its_future_drops_the_handler_without_a_runtime() {
    let messager = messager();
    let message = WaitForRelease::default();
    let mut future = Box::pin(messager.send(message.clone()));
    assert!(message.started.notified().now_or_never().is_none());

    // Poll directly without a runtime: execution must remain in the calling future.
    assert!(future.as_mut().poll(&mut Context::from_waker(noop_waker_ref())).is_pending());
    assert!(message.started.notified().now_or_never().is_some());
    assert!(message.dropped.notified().now_or_never().is_none());

    drop(future);
    assert!(message.dropped.notified().now_or_never().is_some());
}

#[tokio::test(start_paused = true)]
async fn spawn_handle_can_abort_and_join_the_in_flight_message() {
    let message = WaitForRelease::default();
    let handle = messager().spawn(message.clone());
    tokio::time::timeout(Duration::from_secs(1), message.started.notified()).await.unwrap();
    assert!(message.dropped.notified().now_or_never().is_none());

    handle.abort();
    let error = tokio::time::timeout(Duration::from_secs(1), handle).await.unwrap().unwrap_err();
    assert!(error.is_cancelled());
    assert!(message.dropped.notified().now_or_never().is_some());
}

#[tokio::test(start_paused = true)]
async fn dropping_the_spawn_handle_detaches_the_message() {
    let message = WaitForRelease::default();
    let handle = messager().spawn(message.clone());
    tokio::time::timeout(Duration::from_secs(1), message.started.notified()).await.unwrap();

    drop(handle);
    tokio::task::yield_now().await;
    assert!(message.dropped.notified().now_or_never().is_none());
    message.release.notify_one();
    tokio::time::timeout(Duration::from_secs(1), message.dropped.notified()).await.unwrap();
}
