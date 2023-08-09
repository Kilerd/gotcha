//! cron based task system
//! #[cron("")]
//! async fn task_one() {
//!  todo!()   
//! }
use std::str::FromStr;

use chrono::Utc;
use cron::Schedule;
use tracing::info;

async fn my_task() {
    info!("hello task");
}

pub async fn cron_proc_macro_wrapper() {
    let expression = "0   30   9,12,15     1,15       May-Aug  Mon,Wed,Fri  2018/2";
    let schedule = Schedule::from_str(expression).unwrap();
    let mut scheduler = schedule.upcoming(Utc);
    while let Some(next_trigger_time) = scheduler.next() {
        let now = Utc::now();
        let duration = next_trigger_time - now;
        tokio::time::sleep(duration.to_std().unwrap()).await;
        my_task().await
    }
}

pub async fn interval_proc_macro_wrapper() {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        let _tick = interval.tick().await;
        my_task().await
    }
}
