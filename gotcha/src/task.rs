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
//! use gotcha::{GotchaContext, TaskScheduler};
//! use std::time::Duration;
//!
//! // Create a task scheduler
//! let scheduler = TaskScheduler::new(context);
//!
//! // Schedule a cron task
//! scheduler.cron("daily-cleanup", "0 0 * * *", |ctx| async move {
//!     // Task implementation
//! });
//!
//! // Schedule an interval task
//! scheduler.interval("heartbeat", Duration::from_secs(60), |ctx| async move {
//!     // Task implementation  
//! });
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

    pub fn cron<F, FF>(&self, name: impl AsRef<str>, expression: String, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        info!("starting cron task: {}", name.as_ref());
        tokio::spawn(cron_proc_macro_wrapper(self.context.clone(), expression, task));
    }

    pub fn interval<F, FF>(&self, name: impl AsRef<str>, interval: std::time::Duration, task: F)
    where
        F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        info!("starting interval task: {}", name.as_ref());
        tokio::spawn(interval_proc_macro_wrapper(self.context.clone(), interval, task));
    }
}

pub async fn cron_proc_macro_wrapper<T1, T2, F, FF>(context: GotchaContext<T1, T2>, expression: String, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    let schedule: Schedule = Schedule::from_str(&expression).unwrap();
    let scheduler = schedule.upcoming(Utc);

    for next_trigger_time in scheduler {
        let now = Utc::now();
        let duration = next_trigger_time - now;
        tokio::time::sleep(duration.to_std().unwrap()).await;
        let t = task(context.clone());
        t.await
    }
}

pub async fn interval_proc_macro_wrapper<T1, T2, F, FF>(context: GotchaContext<T1, T2>, interval: std::time::Duration, task: F)
where
    T1: Clone + Send + Sync + 'static,
    T2: Clone + Send + Sync + 'static + Serialize + for<'de> Deserialize<'de> + Default,
    F: Fn(GotchaContext<T1, T2>) -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    let mut interval = tokio::time::interval(interval);
    loop {
        let _tick = interval.tick().await;
        let t = task(context.clone());
        t.await
    }
}
