//! # Task Module
//!
//! This module runs background work that the application owns: it starts together with serving,
//! sees the shutdown token through its context, and is joined before cleanup hooks run.
//!
//! ## Features
//!
//! - Long-running work such as queue consumers and watchers ([`TaskScheduler::spawn`])
//! - Fixed interval scheduling ([`TaskScheduler::interval`])
//! - Cron expression based scheduling (`TaskScheduler::cron`, requires the `task` feature)
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
//!     // Run a loop until shutdown
//!     scheduler.spawn("consumer", |ctx| async move {
//!         let shutdown = ctx.shutdown();
//!         shutdown.cancelled().await;
//!         // Flush what the loop holds, then return
//!     });
//!
//!     // Schedule an interval task
//!     scheduler.interval("heartbeat", Duration::from_secs(60), |_ctx| async move {
//!         // Task implementation
//!     });
//!
//!     // Schedule a cron task (the expression is a `String`; requires the `task` feature)
//!     # #[cfg(feature = "task")]
//!     scheduler.cron("daily-cleanup", "0 0 0 * * *".to_string(), |_ctx| async move {
//!         // Task implementation
//!     });
//! }
//! ```
//!
//! Tasks have access to the application context and can be used for:
//! - Queue consumers and file watchers
//! - Periodic cleanup jobs
//! - Data synchronization
//! - Health checks
//! - Background processing
//! - Scheduled notifications
//!

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
#[cfg(feature = "task")]
use std::str::FromStr;
use std::time::Duration;

#[cfg(feature = "task")]
use chrono::{DateTime, Utc};
#[cfg(feature = "task")]
use cron::Schedule;
use futures_util::FutureExt;
use tokio::task::{Id, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{GotchaContext, GotchaError, GotchaResult};

/// A registered task: its owned future and the name that shutdown diagnostics report.
pub(crate) struct ScheduledTask {
    name: String,
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
}

pub(crate) const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for aborted tasks to stop. A task blocking its thread never observes the
/// abort, so shutdown continues without it instead of waiting forever.
const ABORT_GRACE: Duration = Duration::from_secs(1);

/// One registration that cannot be scheduled. The message names only the task (and expression);
/// the cron parse error, when there is one, becomes the error source of [`RegistrationErrors`].
#[derive(Debug)]
struct RegistrationFailure {
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl fmt::Display for RegistrationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// All rejected registrations, reported as one startup error.
#[derive(Debug)]
struct RegistrationErrors(Vec<RegistrationFailure>);

impl fmt::Display for RegistrationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task registration failed: ")?;
        for (index, failure) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            formatter.write_str(&failure.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for RegistrationErrors {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // The cron parser produces the only source in this chain; zero intervals have none.
        self.0.iter().find_map(|failure| failure.source.as_deref()).map(|source| source as _)
    }
}

/// Registers tasks without starting them. Applications start all registrations together after
/// successful initialization. Standalone callers must retain the owner returned by [`Self::start`].
pub struct TaskScheduler<T1: Clone + Send + Sync + 'static, T2: Clone + Send + Sync + 'static> {
    context: GotchaContext<T1, T2>,
    shutdown: CancellationToken,
    tasks: Vec<ScheduledTask>,
    registration_errors: Vec<RegistrationFailure>,
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
            registration_errors: Vec::new(),
        }
    }

    /// Register a cron task. An invalid expression is recorded as a registration error, which
    /// fails application startup instead of starting a partial schedule. Standalone schedulers
    /// log the error and skip the task. Each trigger is computed from the current time and the
    /// last trigger that fired, so an execution that overruns a trigger skips it instead of
    /// running again immediately, and a wall-clock step back cannot fire the same trigger twice.
    #[cfg(feature = "task")]
    #[cfg_attr(docsrs, doc(cfg(feature = "task")))]
    pub fn cron<F, FF>(&mut self, name: impl AsRef<str>, expression: String, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        let schedule = match Schedule::from_str(&expression) {
            Ok(schedule) => schedule,
            Err(source) => {
                self.registration_errors.push(RegistrationFailure {
                    message: format!("cron task {name:?} has an invalid schedule {expression:?}"),
                    source: Some(Box::new(source)),
                });
                return;
            }
        };
        self.tasks.push(ScheduledTask {
            name: name.clone(),
            future: Box::pin(run_cron(self.context.clone(), schedule, name, task, self.shutdown.clone())),
        });
    }

    /// Register a task with an immediate first execution, then repeat at the given interval.
    /// A zero interval is recorded as a registration error, which fails application startup
    /// instead of starting a partial schedule. Standalone schedulers log the error and skip the
    /// task. Executions of the same task never overlap. A slow execution runs the already-due
    /// tick once when it finishes and then resumes at the next period boundary; the ticks it
    /// missed are not replayed.
    pub fn interval<F, FF>(&mut self, name: impl AsRef<str>, interval: Duration, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        if interval.is_zero() {
            self.registration_errors.push(RegistrationFailure {
                message: format!("interval task {name:?} has a zero interval"),
                source: None,
            });
            return;
        }
        self.tasks.push(ScheduledTask {
            name: name.clone(),
            future: Box::pin(run_interval(self.context.clone(), interval, name, task, self.shutdown.clone())),
        });
    }

    /// Register one long-running or one-off future, such as a queue consumer or a watcher.
    ///
    /// The application owns it like every other task. An application starts it when serving
    /// starts, and not at all if a registration fails. A standalone scheduler starts it in
    /// [`Self::start`], which logs and skips invalid registrations and starts the rest. Returning
    /// before shutdown is not an error and the future does not run again. A panic is logged and
    /// ends only this task: other tasks and the server keep running.
    ///
    /// When shutdown is requested, [`GotchaContext::shutdown`] fires and the future has until the
    /// task shutdown timeout to return; at the deadline it is aborted, see
    /// [`RunningTasks::shutdown`]. HTTP connections drain at the same time, not first, and cleanup
    /// hooks run after both. So a consumer can stop while a handler is still running, and a job
    /// that handler enqueues afterwards is never consumed. As below, close the queue before the
    /// last drain, and have handlers check their `send` and refuse the work (for example with
    /// `503`) when it fails: a `send` that succeeded is handled, unless the deadline aborts the
    /// consumer first.
    ///
    /// ```no_run
    /// use gotcha::Gotcha;
    /// use tokio::sync::mpsc;
    ///
    /// let (jobs, mut queue) = mpsc::channel::<String>(16);
    /// let app = Gotcha::new().tasks(move |scheduler| {
    ///     scheduler.spawn("consumer", |ctx| async move {
    ///         let shutdown = ctx.shutdown();
    ///         loop {
    ///             tokio::select! {
    ///                 _ = shutdown.cancelled() => break,
    ///                 Some(job) = queue.recv() => println!("{job}"),
    ///             }
    ///         }
    ///         // Refuse new jobs and finish the queued ones; cleanup hooks run after this returns.
    ///         queue.close();
    ///         while let Some(job) = queue.recv().await {
    ///             println!("{job}");
    ///         }
    ///     });
    /// });
    /// # drop(jobs);
    /// ```
    pub fn spawn<F, FF>(&mut self, name: impl AsRef<str>, task: F)
    where
        F: FnOnce(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        self.tasks.push(ScheduledTask {
            name: name.clone(),
            future: Box::pin(run_spawned(self.context.clone(), name, task)),
        });
    }

    /// Start the registered tasks on Tokio. Dropping the returned owner aborts them.
    ///
    /// Invalid registrations are logged and skipped here; application startup reports them as
    /// errors instead of starting a partial schedule.
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
        for error in &self.registration_errors {
            tracing::error!("{error}; task not registered");
        }
        RunningTasks::start(self.tasks, self.shutdown)
    }

    /// The pending tasks, or the registration errors that must fail startup.
    pub(crate) fn into_tasks(self) -> GotchaResult<Vec<ScheduledTask>> {
        if self.registration_errors.is_empty() {
            return Ok(self.tasks);
        }
        Err(GotchaError::Other(Box::new(RegistrationErrors(self.registration_errors))))
    }
}

/// Owns all running tasks: spawned futures and schedules, including their current executions.
/// Dropping this value requests immediate abortion; [`Self::shutdown`] also waits for cleanup.
/// Tasks spawned independently by application code (`tokio::spawn`) are outside this owner.
#[must_use = "dropping the task owner aborts all scheduled tasks"]
pub struct RunningTasks {
    tasks: JoinSet<()>,
    // The tasks not joined yet, for shutdown diagnostics.
    names: HashMap<Id, String>,
    shutdown: CancellationToken,
}

impl RunningTasks {
    pub(crate) fn start(pending: Vec<ScheduledTask>, shutdown: CancellationToken) -> Self {
        let mut tasks = JoinSet::new();
        let mut names = HashMap::new();
        for task in pending {
            names.insert(tasks.spawn(task.future).id(), task.name);
        }
        Self { tasks, names, shutdown }
    }

    /// Stop new executions and wait up to `timeout` for spawned futures and current executions to
    /// complete.
    /// Also cancels the scheduler's shutdown token, which is a child of the context token for
    /// schedulers built with [`TaskScheduler::new`], releasing executions that wait on it without
    /// cancelling that token or other schedulers sharing it.
    /// At the deadline, abort unfinished tasks and wait up to one second for their futures to be
    /// dropped. Cancelling this future also aborts the tasks.
    ///
    /// An abort takes effect when a task yields. A task blocking its thread is named in a warning
    /// and left behind so that this returns, and application cleanup can run, but it keeps its
    /// thread until it yields and the runtime waits for that thread when it is dropped. On a
    /// single-threaded runtime it also blocks this wait. Application-created detached tasks are
    /// outside this owner.
    pub async fn shutdown(mut self, timeout: Duration) {
        self.shutdown.cancel();
        if tokio::time::timeout(timeout, self.join_all()).await.is_ok() {
            return;
        }
        tracing::warn!("background tasks {:?} exceeded shutdown timeout {timeout:?}; aborting them", self.running());
        self.tasks.abort_all();
        if tokio::time::timeout(ABORT_GRACE, self.join_all()).await.is_err() {
            tracing::warn!(
                "background tasks {:?} still running {ABORT_GRACE:?} after abort; continuing shutdown",
                self.running()
            );
        }
    }

    async fn join_all(&mut self) {
        while let Some(result) = self.tasks.join_next_with_id().await {
            match result {
                Ok((id, ())) => {
                    self.names.remove(&id);
                }
                Err(error) => {
                    let name = self.names.remove(&error.id());
                    if !error.is_cancelled() {
                        tracing::error!("background task {name:?} failed: {error}");
                    }
                }
            }
        }
    }

    fn running(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.names.values().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

// Run the callback and its future in the owned task. Catch both construction and polling panics
// without spawning an execution that could outlive the task. Schedules pass a mutable borrow, which
// retains F: Send without requiring F: Sync.
async fn run_supervised<T1, T2, F, FF>(name: &str, context: GotchaContext<T1, T2>, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: FnOnce(GotchaContext<T1, T2>) -> FF + Send,
    FF: Future<Output = ()> + Send,
{
    if let Err(panic) = AssertUnwindSafe(async move { task(context).await }).catch_unwind().await {
        let message = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        tracing::error!("background task {name:?} panicked: {message}");
    }
}

async fn run_spawned<T1, T2, F, FF>(context: GotchaContext<T1, T2>, name: String, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static,
    F: FnOnce(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    tracing::info!("starting background task: {name}");
    run_supervised(&name, context, task).await;
}

/// The first trigger after `now` and after the trigger that already fired. Recomputing it every
/// loop iteration keeps a slow execution from replaying triggers that elapsed while it ran, and
/// anchoring to `last` keeps a wall-clock step back (or timer/wall-clock drift) from firing the
/// same trigger twice.
#[cfg(feature = "task")]
fn next_cron_trigger(schedule: &Schedule, now: DateTime<Utc>, last: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    let reference = last.map_or(now, |last| now.max(last));
    schedule.after(&reference).next()
}

#[cfg(feature = "task")]
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
#[cfg(feature = "task")]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
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
