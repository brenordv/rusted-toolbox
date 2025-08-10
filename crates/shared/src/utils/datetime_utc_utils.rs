use chrono::{DateTime, Local, TimeDelta, Utc};

const DATE_TIME_TO_SAFE_FILENAME_FORMAT: &str = "%Y-%m-%d--%H-%M-%S--%f";

pub trait DateTimeUtcUtils {
    fn get_elapsed_time(&self) -> TimeDelta;
}

pub trait DateTimeUtilsExt {
    fn get_datetime_as_filename_safe_string(&self) -> String;
}

impl DateTimeUtcUtils for DateTime<Utc> {
    fn get_elapsed_time(&self) -> TimeDelta {
        Utc::now() - self
    }
}

impl DateTimeUtilsExt for DateTime<Local> {
    fn get_datetime_as_filename_safe_string(&self) -> String {
        self.format(DATE_TIME_TO_SAFE_FILENAME_FORMAT).to_string()
    }
}

impl DateTimeUtilsExt for DateTime<Utc> {
    fn get_datetime_as_filename_safe_string(&self) -> String {
        self.format(DATE_TIME_TO_SAFE_FILENAME_FORMAT).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_datetime_as_filename_safe_string_utc() {
        const TEST_DT_FORMAT: &str = "1978-11-25T14:15:16.987654321Z";
        const EXPECTED_DATETIME: &str = "1978-11-25--14-15-16--987654321";

        let dt: DateTime<Utc> = TEST_DT_FORMAT
            .parse::<DateTime<Utc>>()
            .expect("Invalid date format");

        let filename_safe_dt = dt.get_datetime_as_filename_safe_string();

        assert_eq!(filename_safe_dt, EXPECTED_DATETIME);
    }

    #[test]
    fn test_get_datetime_as_filename_safe_string_local() {
        //This test is not really portable, because of timezones.
        let dt: DateTime<Local> = Local::now();

        let filename_safe_dt = dt.get_datetime_as_filename_safe_string();
        assert!(filename_safe_dt.len() > 0);
    }
}
