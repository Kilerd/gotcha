//! # Task Module
//!
//! This module provides task scheduling capabilities for Gotcha web applications.
//! It supports both cron-based and interval-based task scheduling.
//!
//! ## Features
//!
//! - Cron expression based scheduling
//! - Fixed interval scheduling
//! - Async task execution
//! - Access to application context in tasks
//!
//! ## Examples
//!
//! ```rust,no_run
//! use gotcha::TaskScheduler;
//! use std::time::Duration;
//!
//! # #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
//! # struct Config {}
//! fn setup(scheduler: &TaskScheduler<(), Config>) {
//!     // Schedule a cron task (the expression is a `String`)
//!     scheduler.cron("daily-cleanup", "0 0 * * *".to_string(), |_ctx| async move {
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
use std::str::FromStr;

use chrono::Utc;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::GotchaContext;

pub struct TaskScheduler<T1: Clone + Send + Sync + 'static, T2: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default> {
    context: GotchaContext<T1, T2>,
}

impl<T1, T2> TaskScheduler<T1, T2>
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
{
    pub fn new(context: GotchaContext<T1, T2>) -> Self {
        Self { context }
    }

    /// Schedule a task on a cron expression.
    ///
    /// An invalid expression is reported (via `tracing::error!`) and the task is
    /// simply not started, instead of panicking a detached task at the first tick.
    pub fn cron<F, FF>(&self, name: impl AsRef<str>, expression: String, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        let schedule = match Schedule::from_str(&expression) {
            Ok(schedule) => schedule,
            Err(e) => {
                tracing::error!("cron task {name:?} has an invalid schedule {expression:?}: {e}; task not started");
                return;
            }
        };
        info!("starting cron task: {name}");
        tokio::spawn(cron_proc_macro_wrapper(self.context.clone(), schedule, name, task));
    }

    pub fn interval<F, FF>(&self, name: impl AsRef<str>, interval: std::time::Duration, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        let name = name.as_ref().to_string();
        info!("starting interval task: {name}");
        tokio::spawn(interval_proc_macro_wrapper(self.context.clone(), interval, name, task));
    }
}

/// Run one task execution under supervision: if it panics, log it and keep the
/// scheduler loop alive instead of silently killing the whole task.
async fn run_supervised<FF>(name: &str, fut: FF)
where
    FF: Future<Output = ()> + Send + 'static,
{
    if let Err(join_error) = tokio::spawn(fut).await {
        tracing::error!("scheduled task {name:?} panicked: {join_error}");
    }
}

pub async fn cron_proc_macro_wrapper<T1, T2, F, FF>(context: GotchaContext<T1, T2>, schedule: Schedule, name: String, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    for next_trigger_time in schedule.upcoming(Utc) {
        let now = Utc::now();
        // A trigger computed in the past (clock skew, or a long previous run) would make
        // `to_std()` fail — run immediately in that case rather than panicking.
        let wait = (next_trigger_time - now).to_std().unwrap_or(std::time::Duration::ZERO);
        tokio::time::sleep(wait).await;
        run_supervised(&name, task(context.clone())).await;
    }
}

pub async fn interval_proc_macro_wrapper<T1, T2, F, FF>(context: GotchaContext<T1, T2>, interval: std::time::Duration, name: String, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    let mut interval = tokio::time::interval(interval);
    loop {
        interval.tick().await;
        run_supervised(&name, task(context.clone())).await;
    }
}
