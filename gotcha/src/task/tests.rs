use super::*;
use crate::ConfigWrapper;
use std::cell::Cell;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, Notify};

fn scheduler() -> TaskScheduler<(), ()> {
    TaskScheduler::new(GotchaContext {
        state: (),
        config: ConfigWrapper::default(),
    })
}

struct Dropped(mpsc::UnboundedSender<()>);
impl Drop for Dropped {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn registration_is_inert_and_can_be_discarded_without_a_runtime() {
    let mut scheduler = scheduler();
    let (dropped, mut drops) = mpsc::unbounded_channel();
    let resource = Dropped(dropped);
    scheduler.interval("not-started", Duration::from_secs(1), move |_| -> std::future::Ready<()> {
        let _ = &resource;
        panic!("registration must not call the task");
    });
    scheduler.cron("invalid", "not a schedule".into(), |_| async {});
    scheduler.interval("zero", Duration::ZERO, |_| async {});
    drop(scheduler);
    assert_eq!(drops.try_recv(), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn graceful_shutdown_finishes_current_execution_without_starting_another() {
    let mut scheduler = scheduler();
    let count = Arc::new(AtomicUsize::new(0));
    let finish = Arc::new(Notify::new());
    let (started, mut starts) = mpsc::unbounded_channel();
    scheduler.interval("slow", Duration::from_secs(1), {
        let count = count.clone();
        let finish = finish.clone();
        move |_| {
            let finish = finish.clone();
            let count = count.clone();
            let started = started.clone();
            async move {
                started.send(()).unwrap();
                finish.notified().await;
                count.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let running = scheduler.start();
    starts.recv().await.unwrap();
    let shutdown = tokio::spawn(running.shutdown(Duration::from_secs(30)));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    assert!(!shutdown.is_finished());
    finish.notify_one();
    shutdown.await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(starts.recv().await, None);
}

#[tokio::test(start_paused = true)]
async fn timeout_drop_and_cancelled_shutdown_release_in_flight_executions() {
    for cron in [false, true] {
        for mode in ["timeout", "drop", "cancel-shutdown"] {
            let mut scheduler = scheduler();
            let (started, mut starts) = mpsc::unbounded_channel();
            let (dropped, mut drops) = mpsc::unbounded_channel();
            let task = move |_| {
                let started = started.clone();
                let dropped = dropped.clone();
                async move {
                    let _resource = Dropped(dropped);
                    started.send(()).unwrap();
                    std::future::pending::<()>().await;
                }
            };
            if cron {
                scheduler.cron("blocked-cron", "* * * * * * *".into(), task);
            } else {
                scheduler.interval("blocked-interval", Duration::from_secs(1), task);
            }
            let running = scheduler.start();
            starts.recv().await.unwrap();
            match mode {
                "timeout" => {
                    let before = tokio::time::Instant::now();
                    running.shutdown(Duration::from_secs(10)).await;
                    assert_eq!(before.elapsed(), Duration::from_secs(10));
                    // Explicit shutdown has joined the cancelled future before returning.
                    assert_eq!(drops.try_recv(), Ok(()));
                }
                "drop" => {
                    drop(running);
                    drops.recv().await.unwrap();
                }
                _ => {
                    let shutdown = tokio::spawn(running.shutdown(Duration::from_secs(30)));
                    tokio::task::yield_now().await;
                    shutdown.abort();
                    assert!(shutdown.await.unwrap_err().is_cancelled());
                    drops.recv().await.unwrap();
                }
            }
            assert_eq!(starts.recv().await, None);
        }
    }
}

#[derive(Clone)]
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

#[tokio::test(start_paused = true)]
async fn construction_and_polling_panics_are_logged_and_the_schedule_continues() {
    use tracing::instrument::WithSubscriber;
    let mut scheduler = scheduler();
    let (completed, mut completions) = mpsc::unbounded_channel();
    let attempts = Cell::new(0); // The callback is Send but deliberately not Sync.
    scheduler.interval("recovering", Duration::from_secs(1), move |_| {
        let attempt = attempts.get();
        attempts.set(attempt + 1);
        assert_ne!(attempt, 0, "construction panic");
        let completed = completed.clone();
        async move {
            assert_ne!(attempt, 1, "polling panic");
            completed.send(attempt).unwrap();
        }
    });
    let logs = LogBuffer(Arc::default());
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let dispatch = tracing::Dispatch::new(subscriber);
    // Attach the capture subscriber to the scheduled future so it follows Tokio polling.
    scheduler.tasks = scheduler
        .tasks
        .into_iter()
        .map(|task| Box::pin(task.with_subscriber(dispatch.clone())) as ScheduledTask)
        .collect();
    let running = scheduler.start();
    assert_eq!(completions.recv().await, Some(2));
    running.shutdown(Duration::from_secs(1)).await;
    let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("recovering") && logs.contains("construction panic") && logs.contains("polling panic"),
        "{logs}"
    );
}
