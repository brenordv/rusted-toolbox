use chrono::{DateTime, TimeDelta, Utc};

pub trait DateTimeUtcUtils {
    fn get_elapsed_time(&self) -> TimeDelta;
}

impl DateTimeUtcUtils for DateTime<Utc> {
    fn get_elapsed_time(&self) -> TimeDelta {
        Utc::now() - self
    }
}
