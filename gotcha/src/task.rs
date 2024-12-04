use std::future::Future;
use std::str::FromStr;

use chrono::Utc;
use cron::Schedule;
use tracing::info;

pub struct TaskScheduler {}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn cron<F, FF>(&self, name: impl AsRef<str>, expression: String, task: F)
    where
        F: Fn() -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        info!("starting cron task: {}", name.as_ref());
        tokio::spawn(cron_proc_macro_wrapper(expression, task));
    }

    pub fn interval<F, FF>(&self, name: impl AsRef<str>, interval: std::time::Duration, task: F)
    where
        F: Fn() -> FF + Send + 'static,
        FF: Future<Output = ()> + Send + 'static,
    {
        info!("starting interval task: {}", name.as_ref());
        tokio::spawn(interval_proc_macro_wrapper(interval, task));
    }
}

pub async fn cron_proc_macro_wrapper<F, FF>(expression: String, task: F)
where
    F: Fn() -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    let schedule: Schedule = Schedule::from_str(&expression).unwrap();
    let scheduler = schedule.upcoming(Utc);
    for next_trigger_time in scheduler {
        let now = Utc::now();
        let duration = next_trigger_time - now;
        tokio::time::sleep(duration.to_std().unwrap()).await;
        let t = task();
        t.await
    }
}

pub async fn interval_proc_macro_wrapper<F, FF>(interval: std::time::Duration, task: F)
where
    F: Fn() -> FF + Send + 'static,
    FF: Future<Output = ()> + Send + 'static,
{
    let mut interval = tokio::time::interval(interval);
    loop {
        let _tick = interval.tick().await;
        let t = task();
        t.await
    }
}
