//! # Task Module
//!
//! This module provides task scheduling capabilities for Gotcha web applications.
//! It supports both cron-based and interval-based task scheduling.
//!
//! ## Features
//!
//! - Cron expression based scheduling
//! - Fixed interval scheduling
//! - Async task execution with application-owned shutdown
//! - Access to application context in tasks
//!
//! ## Examples
//!
//! ```rust,no_run
//! use gotcha::TaskScheduler;
//! use std::time::Duration;
//!
//! # #[derive(Clone)]
//! # struct Config {}
//! fn setup(scheduler: &mut TaskScheduler<(), Config>) {
//!     // Schedule a cron task (the expression is a `String`)
//!     scheduler.cron("daily-cleanup", "0 0 0 * * *".to_string(), |_ctx| async move {
//!         // Task implementation
//!     });
//!
//!     // Schedule an interval task
//!     scheduler.interval("heartbeat", Duration::from_secs(60), |_ctx| async move {
//!         // Task implementation
//!     });
//! }
//! ```
//!
//! Tasks have access to the application context and can be used for:
//! - Periodic cleanup jobs
//! - Data synchronization
//! - Health checks
//! - Background processing
//! - Scheduled notifications
//!

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use futures_util::FutureExt;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::GotchaContext;

pub(crate) type ScheduledTask = Pin<Box<dyn Future<Output = ()> + Send>>;
pub(crate) const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// Registers tasks without starting them. Applications start all registrations together after
/// successful initialization. Standalone callers must retain the owner returned by [`Self::start`].
pub struct TaskScheduler<T1: Clone + Send + Sync + 'static, T2: Clone + Send + Sync + 'static> {
    context: GotchaContext<T1, T2>,
    shutdown: CancellationToken,
    tasks: Vec<ScheduledTask>,
}

impl<T1, T2> TaskScheduler<T1, T2>
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
{
    /// Create a registration-only scheduler. This does not require a running Tokio runtime.
    ///
    /// The scheduler uses a child of the context's shutdown token: cancelling the context token
    /// stops the tasks, while [`RunningTasks::shutdown`] only cancels the child, so shutting a
    /// standalone scheduler down neither stops the application nor another scheduler sharing the
    /// context. Tasks receive a context whose [`GotchaContext::shutdown`] is that child token.
    pub fn new(context: GotchaContext<T1, T2>) -> Self {
        let shutdown = context.shutdown().child_token();
        let context = GotchaContext {
            config: context.config,
            state: context.state,
            shutdown: shutdown.clone(),
        };
        Self::with_shutdown(context, shutdown)
    }

    pub(crate) fn with_shutdown(context: GotchaContext<T1, T2>, shutdown: CancellationToken) -> Self {
        Self {
            context,
            shutdown,
            tasks: Vec::new(),
        }
    }

    /// Register a cron task. Invalid expressions are logged and not registered. Each trigger is
    /// computed from the current time and the last trigger that fired, so an execution that
    /// overruns a trigger skips it instead of running again immediately, and a wall-clock step
    /// back cannot fire the same trigger twice.
    pub fn cron<F, FF>(&mut self, name: impl AsRef<str>, expression: String, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        let schedule = match Schedule::from_str(&expression) {
            Ok(schedule) => schedule,
            Err(e) => {
                tracing::error!("cron task {name:?} has an invalid schedule {expression:?}: {e}; task not registered");
                return;
            }
        };
        self.tasks
            .push(Box::pin(run_cron(self.context.clone(), schedule, name, task, self.shutdown.clone())));
    }

    /// Register a task with an immediate first execution, then repeat at the given interval.
    /// A zero interval is logged and not registered. Executions of the same task never overlap.
    /// A slow execution runs the already-due tick once when it finishes and then resumes at the
    /// next period boundary; the ticks it missed are not replayed.
    pub fn interval<F, FF>(&mut self, name: impl AsRef<str>, interval: Duration, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        if interval.is_zero() {
            tracing::error!("interval task {name:?} has a zero interval; task not registered");
            return;
        }
        self.tasks
            .push(Box::pin(run_interval(self.context.clone(), interval, name, task, self.shutdown.clone())));
    }

    /// Start the registered tasks on Tokio. Dropping the returned owner aborts them.
    ///
    /// ```no_run
    /// use gotcha::{ConfigWrapper, GotchaContext, TaskScheduler};
    /// use std::time::Duration;
    /// # async fn example() {
    /// let mut scheduler = TaskScheduler::new(GotchaContext::new((), ConfigWrapper::<()>::default()));
    /// scheduler.interval("cleanup", Duration::from_secs(60), |_| async {});
    /// let running = scheduler.start();
    /// running.shutdown(Duration::from_secs(10)).await;
    /// # }
    /// ```
    ///
    /// Use [`RunningTasks::shutdown`] to stop scheduling and wait for in-flight executions.
    pub fn start(self) -> RunningTasks {
        RunningTasks::start(self.tasks, self.shutdown)
    }

    pub(crate) fn into_tasks(self) -> Vec<ScheduledTask> {
        self.tasks
    }
}

/// Owns all running scheduled tasks, including their current executions.
/// Dropping this value requests immediate abortion; [`Self::shutdown`] also waits for cleanup.
/// Tasks spawned independently by application code are outside this owner.
#[must_use = "dropping the task owner aborts all scheduled tasks"]
pub struct RunningTasks {
    tasks: JoinSet<()>,
    shutdown: CancellationToken,
}

impl RunningTasks {
    pub(crate) fn start(pending: Vec<ScheduledTask>, shutdown: CancellationToken) -> Self {
        let mut tasks = JoinSet::new();
        for task in pending {
            tasks.spawn(task);
        }
        Self { tasks, shutdown }
    }

    /// Stop new executions and wait up to `timeout` for current executions to complete.
    /// Also cancels the scheduler's shutdown token, which is a child of the context token for
    /// schedulers built with [`TaskScheduler::new`], releasing executions that wait on it without
    /// cancelling that token or other schedulers sharing it.
    /// At the deadline, abort unfinished tasks and wait for their futures to be dropped.
    /// Cancelling this future also aborts the tasks. Cancellation requires tasks to yield;
    /// blocking work and application-created detached tasks cannot be forcibly stopped here.
    pub async fn shutdown(mut self, timeout: Duration) {
        self.shutdown.cancel();
        let drain = async {
            while let Some(result) = self.tasks.join_next().await {
                if let Err(error) = result {
                    tracing::error!("scheduled task loop failed: {error}");
                }
            }
        };
        if tokio::time::timeout(timeout, drain).await.is_err() {
            tracing::warn!("scheduled tasks exceeded shutdown timeout {timeout:?}; aborting remaining tasks");
            self.tasks.shutdown().await;
        }
    }
}

// Run the callback and its future in the owned loop. Catch both construction and polling panics
// without spawning an execution that could outlive the loop. A mutable borrow retains F: Send
// without requiring F: Sync.
async fn run_supervised<T1, T2, F, FF>(name: &str, context: GotchaContext<T1, T2>, task: &mut F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send,
    FF: Future<Output = ()> + Send,
{
    if let Err(panic) = AssertUnwindSafe(async move { task(context).await }).catch_unwind().await {
        let message = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        tracing::error!("scheduled task {name:?} panicked: {message}");
    }
}

/// The first trigger after `now` and after the trigger that already fired. Recomputing it every
/// loop iteration keeps a slow execution from replaying triggers that elapsed while it ran, and
/// anchoring to `last` keeps a wall-clock step back (or timer/wall-clock drift) from firing the
/// same trigger twice.
fn next_cron_trigger(schedule: &Schedule, now: DateTime<Utc>, last: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    let reference = last.map_or(now, |last| now.max(last));
    schedule.after(&reference).next()
}

async fn run_cron<T1, T2, F, FF>(context: GotchaContext<T1, T2>, schedule: Schedule, name: String, mut task: F, shutdown: CancellationToken)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    tracing::info!("starting cron task: {name}");
    let mut last = None;
    loop {
        let now = Utc::now();
        let Some(next_trigger_time) = next_cron_trigger(&schedule, now, last) else {
            break;
        };
        let wait = (next_trigger_time - now).to_std().unwrap_or(Duration::ZERO);
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(wait) => {}
        }
        run_supervised(&name, context.clone(), &mut task).await;
        last = Some(next_trigger_time);
    }
}

async fn run_interval<T1, T2, F, FF>(context: GotchaContext<T1, T2>, interval: Duration, name: String, mut task: F, shutdown: CancellationToken)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    tracing::info!("starting interval task: {name}");
    let mut interval = tokio::time::interval(interval);
    // A slow execution must not be followed by one execution per tick it missed.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }
        run_supervised(&name, context.clone(), &mut task).await;
    }
}

/// Low-level cron driver retained for compatibility. The caller owns this future;
/// prefer [`TaskScheduler`] for managed shutdown and waiting. Triggers follow
/// [`TaskScheduler::cron`]: they are recomputed from the current time and missed triggers are
/// skipped.
pub async fn cron_proc_macro_wrapper<T1, T2, F, FF>(context: GotchaContext<T1, T2>, schedule: Schedule, name: String, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    run_cron(context, schedule, name, task, CancellationToken::new()).await;
}

/// Low-level interval driver retained for compatibility. The caller owns this future;
/// prefer [`TaskScheduler`] for managed shutdown and waiting. Ticks follow
/// [`TaskScheduler::interval`]: missed ticks are skipped.
pub async fn interval_proc_macro_wrapper<T1, T2, F, FF>(context: GotchaContext<T1, T2>, interval: Duration, name: String, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    run_interval(context, interval, name, task, CancellationToken::new()).await;
}

#[cfg(test)]
mod tests;
