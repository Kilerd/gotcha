//! Shutdown names the tasks it gives up on. `tracing` caches whether a log line is enabled per
//! process, and a test running alongside can leave that cache stale for a scoped subscriber, so this
//! file holds a single test under a global one.

use gotcha::{ConfigWrapper, GotchaContext, TaskScheduler};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct PanicsWhenDropped;
impl Drop for PanicsWhenDropped {
    fn drop(&mut self) {
        panic!("drop panic");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_continues_past_a_task_blocking_its_thread_and_names_it() {
    let logs = LogBuffer::default();
    let writer = logs.clone();
    tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .init();

    let mut scheduler = TaskScheduler::new(GotchaContext::new((), ConfigWrapper::<()>::default()));
    let (started, mut starts) = mpsc::unbounded_channel();
    let (release, blocked) = std::sync::mpsc::channel::<()>();
    scheduler.spawn("blocks-its-thread", move |_| async move {
        started.send(()).unwrap();
        // Never yields, so neither the shutdown token nor the abort reaches it.
        let _ = blocked.recv();
    });
    scheduler.spawn("ignores-the-token", |_| std::future::pending());
    scheduler.spawn("panics-when-dropped", |_| async {
        let _resource = PanicsWhenDropped;
        std::future::pending::<()>().await;
    });
    scheduler.spawn("finishes", |_| async {});
    let running = scheduler.start();
    starts.recv().await.unwrap();

    let before = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), running.shutdown(Duration::from_millis(20)))
        .await
        .expect("shutdown must not wait for a task blocking its thread");
    assert!(before.elapsed() >= Duration::from_secs(1), "aborted tasks get a grace period");
    release.send(()).unwrap();

    let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains(r#"["blocks-its-thread", "ignores-the-token", "panics-when-dropped"] exceeded"#),
        "{logs}"
    );
    assert!(logs.contains(r#"["blocks-its-thread"] still running"#), "{logs}");
    assert!(logs.contains(r#"background task "panics-when-dropped" failed"#), "{logs}");
}
