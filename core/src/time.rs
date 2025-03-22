use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};

pub fn datetime_now() -> DateTime<Utc> {
    DateTime::from(Utc::now())
}

pub fn datetime_from_timestamp_sec(timestamp_sec: i64) -> Result<DateTime<Utc>> {
    match DateTime::from_timestamp(timestamp_sec, 0) {
        Some(res) => Ok(res),
        None => Err(anyhow!(
            "could not convert timestamp_sec {} to date time",
            timestamp_sec
        )),
    }
}
