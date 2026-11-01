use super::*;
use crate::ConfigWrapper;
use std::cell::Cell;
use std::error::Error;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, Notify};

fn scheduler() -> TaskScheduler<(), ()> {
    TaskScheduler::new(GotchaContext::new((), ConfigWrapper::default()))
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
    let Err(error) = scheduler.into_tasks() else {
        panic!("invalid registrations must be registration errors");
    };
    let message = error.to_string();
    assert!(message.contains("\"invalid\"") && message.contains("\"not a schedule\""), "{message}");
    assert!(message.contains("\"zero\"") && message.contains("zero interval"), "{message}");
    // The top layer names the registrations; the parser's own text belongs to the source layer.
    assert!(!message.contains("Invalid expression"), "{message}");
    let GotchaError::Other(inner) = error else {
        panic!("registration errors must keep their source");
    };
    let source = inner.source().expect("the cron parse error must stay in the error chain");
    assert!(source.to_string().contains("Invalid expression"), "{source}");
    assert_eq!(drops.try_recv(), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn standalone_shutdown_releases_context_waiters_without_cancelling_the_context() {
    let context = GotchaContext::new((), ConfigWrapper::<()>::default());
    let token = context.shutdown();
    let mut scheduler = TaskScheduler::new(context);
    let (started, mut starts) = mpsc::unbounded_channel();
    scheduler.interval("waits-for-shutdown", Duration::from_secs(1), move |ctx| {
        let started = started.clone();
        async move {
            started.send(()).unwrap();
            ctx.shutdown().cancelled().await;
        }
    });
    let running = scheduler.start();
    starts.recv().await.unwrap();
    // The execution waits on the scheduler's child token, so shutdown joins it instead of
    // running out the task deadline, but the context's own token is not cancelled.
    tokio::time::timeout(Duration::from_secs(1), running.shutdown(Duration::from_secs(60)))
        .await
        .expect("shutdown must release executions waiting on the scheduler token");
    assert!(!token.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn standalone_shutdown_does_not_stop_another_scheduler_sharing_the_context() {
    let context = GotchaContext::new((), ConfigWrapper::<()>::default());
    let token = context.shutdown();
    let (started, mut starts) = mpsc::unbounded_channel();
    let second_runs = Arc::new(AtomicUsize::new(0));
    let mut first = TaskScheduler::new(context.clone());
    first.interval("first", Duration::from_secs(1), |_| async {});
    let mut second = TaskScheduler::new(context.clone());
    second.interval("second", Duration::from_secs(1), {
        let second_runs = second_runs.clone();
        let started = started.clone();
        move |_| {
            let second_runs = second_runs.clone();
            let started = started.clone();
            async move {
                started.send(()).unwrap();
                second_runs.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let first = first.start();
    let second = second.start();
    starts.recv().await.unwrap();
    first.shutdown(Duration::from_secs(1)).await;
    assert!(!token.is_cancelled());
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert!(
        second_runs.load(Ordering::SeqCst) >= 2,
        "shutting down one scheduler must not stop another on the same context"
    );
    second.shutdown(Duration::from_secs(1)).await;
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
async fn interval_skips_ticks_missed_by_a_slow_execution() {
    let mut scheduler = scheduler();
    let (started, mut starts) = mpsc::unbounded_channel();
    let executions = Arc::new(AtomicUsize::new(0));
    scheduler.interval("slow", Duration::from_secs(1), {
        let executions = executions.clone();
        move |_| {
            let executions = executions.clone();
            let started = started.clone();
            async move {
                started.send(tokio::time::Instant::now()).unwrap();
                if executions.fetch_add(1, Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_millis(3_500)).await;
                }
            }
        }
    });
    let running = scheduler.start();
    let first = starts.recv().await.unwrap();
    let second = starts.recv().await.unwrap();
    let third = starts.recv().await.unwrap();
    running.shutdown(Duration::from_secs(1)).await;
    assert!(
        second.duration_since(first) >= Duration::from_millis(3_500),
        "the slow execution must hold the loop: {second:?} - {first:?}",
    );
    // The first tick fires immediately and its execution runs until 3.5s. The overdue tick runs
    // once when the execution ends, but the next one resumes at the 4s boundary instead of
    // replaying the ticks at 1s, 2s and 3s back to back.
    assert!(
        third.duration_since(second) >= Duration::from_millis(400),
        "missed interval ticks must be skipped: {third:?} - {second:?}",
    );
}

#[test]
fn cron_trigger_is_strictly_after_now() {
    let schedule = Schedule::from_str("* * * * * * *").unwrap();
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
    // On a trigger instant the next trigger is one period later, not `now` again.
    assert_eq!(next_cron_trigger(&schedule, now, None), Some(now + chrono::Duration::seconds(1)));
    // Between triggers only the remaining part of the period is waited for.
    let halfway = now + chrono::Duration::milliseconds(500);
    assert_eq!(next_cron_trigger(&schedule, halfway, None), Some(now + chrono::Duration::seconds(1)));
    assert!(next_cron_trigger(&schedule, now, None).unwrap() > now);
}

#[test]
fn cron_trigger_does_not_repeat_after_a_clock_step_back() {
    let schedule = Schedule::from_str("* * * * * * *").unwrap();
    let fired = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
    // The sleep can wake before the wall clock reaches the trigger that already fired, for
    // example after an NTP step or timer drift. That trigger must not fire a second time.
    let now = fired - chrono::Duration::milliseconds(1);
    assert_eq!(next_cron_trigger(&schedule, now, Some(fired)), Some(fired + chrono::Duration::seconds(1)));
}

#[tokio::test]
async fn cron_skips_triggers_missed_by_a_slow_execution() {
    let mut scheduler = scheduler();
    let (started, mut starts) = mpsc::unbounded_channel();
    let (ended, mut ends) = mpsc::unbounded_channel();
    let executions = Arc::new(AtomicUsize::new(0));
    scheduler.cron("slow", "* * * * * * *".into(), {
        let executions = executions.clone();
        move |_| {
            let executions = executions.clone();
            let started = started.clone();
            let ended = ended.clone();
            async move {
                started.send(std::time::Instant::now()).unwrap();
                // Outlives the following one-second trigger.
                if executions.fetch_add(1, Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_millis(1_100)).await;
                    ended.send(std::time::Instant::now()).unwrap();
                }
            }
        }
    });
    let running = scheduler.start();
    let _first = tokio::time::timeout(Duration::from_secs(10), starts.recv()).await.unwrap().unwrap();
    let end = tokio::time::timeout(Duration::from_secs(10), ends.recv()).await.unwrap().unwrap();
    let second = tokio::time::timeout(Duration::from_secs(10), starts.recv()).await.unwrap().unwrap();
    running.shutdown(Duration::from_secs(1)).await;
    // The first execution starts at a trigger and ends 1.1s later. The old iterator replayed the
    // trigger that elapsed meanwhile, so the next execution began immediately; it must now wait
    // for the next trigger strictly after the execution, which is 0.9s away when the execution
    // starts near its trigger.
    assert!(
        second.duration_since(end) >= Duration::from_millis(50),
        "a slow cron execution must not be followed by a catch-up run: {second:?} - {end:?}",
    );
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
