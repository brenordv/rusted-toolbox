use crate::models::MockOptions;
use anyhow::Result;
use chrono::{Duration, Local, NaiveTime, Timelike, Utc};
use rand::RngExt;

/// Generate a random date
pub fn generate_date(options: &MockOptions) -> Result<String> {
    let today = Local::now().date_naive();
    let range_years = options.range.unwrap_or(50) as i64;

    let date = if options.past {
        let days_back = rand::rng().random_range(1..(range_years * 365));
        today - Duration::days(days_back)
    } else if options.future {
        let days_forward = rand::rng().random_range(1..(range_years * 365));
        today + Duration::days(days_forward)
    } else {
        // Random date within range (past or future)
        let days_offset = rand::rng().random_range(-(range_years * 365)..(range_years * 365));
        today + Duration::days(days_offset)
    };

    Ok(date.format("%Y-%m-%d").to_string())
}

/// Generate a random time.
///
/// With `past`, the result is at or before the current time of day; with
/// `future`, at or after it; with neither, any time of day. When both `past` and
/// `future` are set, `past` wins (matching `generate_date`).
pub fn generate_time(options: &MockOptions) -> Result<String> {
    const LAST_SECOND_OF_DAY: u32 = 86_399;

    let now = Local::now()
        .time()
        .num_seconds_from_midnight()
        .min(LAST_SECOND_OF_DAY);

    let seconds = if options.past {
        rand::rng().random_range(0..=now)
    } else if options.future {
        rand::rng().random_range(now..=LAST_SECOND_OF_DAY)
    } else {
        rand::rng().random_range(0..=LAST_SECOND_OF_DAY)
    };

    let time = NaiveTime::from_num_seconds_from_midnight_opt(seconds, 0)
        .unwrap_or_else(|| Local::now().time());

    Ok(time.format("%H:%M:%S").to_string())
}

/// Generate a random datetime
pub fn generate_datetime(options: &MockOptions) -> Result<String> {
    let now = Local::now();
    let range_years = options.range.unwrap_or(50) as i64;

    let datetime = if options.past {
        let seconds_back = rand::rng().random_range(1..(range_years * 365 * 24 * 3600));
        now - Duration::seconds(seconds_back)
    } else if options.future {
        let seconds_forward = rand::rng().random_range(1..(range_years * 365 * 24 * 3600));
        now + Duration::seconds(seconds_forward)
    } else {
        // Random datetime within range
        let seconds_offset = rand::rng()
            .random_range(-(range_years * 365 * 24 * 3600)..(range_years * 365 * 24 * 3600));
        now + Duration::seconds(seconds_offset)
    };

    Ok(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Generate a random timestamp
pub fn generate_timestamp(options: &MockOptions) -> Result<String> {
    let now = Utc::now();
    let range_years = options.range.unwrap_or(50) as i64;

    let timestamp = if options.past {
        let seconds_back = rand::rng().random_range(1..(range_years * 365 * 24 * 3600));
        now - Duration::seconds(seconds_back)
    } else if options.future {
        let seconds_forward = rand::rng().random_range(1..(range_years * 365 * 24 * 3600));
        now + Duration::seconds(seconds_forward)
    } else {
        // Random timestamp within range
        let seconds_offset = rand::rng()
            .random_range(-(range_years * 365 * 24 * 3600)..(range_years * 365 * 24 * 3600));
        now + Duration::seconds(seconds_offset)
    };

    Ok(timestamp.timestamp().to_string())
}

/// Generate a random hex color
pub fn generate_color_hex(_options: &MockOptions) -> Result<String> {
    let r = rand::rng().random_range(0..256);
    let g = rand::rng().random_range(0..256);
    let b = rand::rng().random_range(0..256);

    Ok(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Generate random RGB color values
pub fn generate_color_rgb(_options: &MockOptions) -> Result<String> {
    let r = rand::rng().random_range(0..256);
    let g = rand::rng().random_range(0..256);
    let b = rand::rng().random_range(0..256);

    Ok(format!("rgb({r}, {g}, {b})"))
}

/// Generate a random integer
pub fn generate_integer(options: &MockOptions) -> Result<String> {
    let min = options.min.unwrap_or(0);
    let max = options.max.unwrap_or(100);

    let value = rand::rng().random_range(min..=max);
    Ok(value.to_string())
}

/// Generate a random float
pub fn generate_float(options: &MockOptions) -> Result<String> {
    let min = options.min.unwrap_or(0) as f64;
    let max = options.max.unwrap_or(100) as f64;
    let precision = options.precision.unwrap_or(2);

    let value: f64 = rand::rng().random_range(min..=max);

    Ok(format!("{:.1$}", value, precision as usize))
}

/// Generate a random car brand name
pub fn generate_car_brand(_options: &MockOptions) -> Result<String> {
    let car_brands = [
        "Toyota",
        "Volkswagen",
        "Ford",
        "Honda",
        "Chevrolet",
        "Mercedes-Benz",
        "BMW",
        "Audi",
        "Hyundai",
        "Nissan",
        "Kia",
        "Subaru",
        "Volvo",
        "Porsche",
        "Lexus",
        "Mazda",
        "Jeep",
        "Ferrari",
        "Lamborghini",
        "Tesla",
        "Jaguar",
        "Land Rover",
        "Peugeot",
        "Renault",
        "Mitsubishi",
        "Fiat",
        "Chrysler",
        "Acura",
        "Infiniti",
    ];

    let brand = car_brands[rand::rng().random_range(0..car_brands.len())];
    Ok(brand.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DataType;
    use chrono::{NaiveDate, NaiveDateTime};

    fn options() -> MockOptions {
        MockOptions {
            data_type: DataType::Integer,
            min: None,
            max: None,
            length: None,
            precision: None,
            age: None,
            past: false,
            future: false,
            range: None,
        }
    }

    #[test]
    fn generate_integer_respects_bounds() {
        let mut opts = options();
        opts.min = Some(5);
        opts.max = Some(10);
        for _ in 0..50 {
            let value: i32 = generate_integer(&opts).unwrap().parse().unwrap();
            assert!((5..=10).contains(&value));
        }
    }

    #[test]
    fn generate_integer_single_value_range() {
        let mut opts = options();
        opts.min = Some(7);
        opts.max = Some(7);
        assert_eq!(generate_integer(&opts).unwrap(), "7");
    }

    #[test]
    fn generate_float_respects_precision_and_bounds() {
        let mut opts = options();
        opts.min = Some(0);
        opts.max = Some(1);
        opts.precision = Some(3);
        for _ in 0..50 {
            let text = generate_float(&opts).unwrap();
            let decimals = text.split('.').nth(1).unwrap();
            assert_eq!(decimals.len(), 3);
            let value: f64 = text.parse().unwrap();
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn generate_color_hex_has_valid_format() {
        let hex = generate_color_hex(&options()).unwrap();
        assert_eq!(hex.len(), 7);
        assert!(hex.starts_with('#'));
        assert!(hex[1..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_color_rgb_has_valid_format() {
        let rgb = generate_color_rgb(&options()).unwrap();
        assert!(rgb.starts_with("rgb("));
        assert!(rgb.ends_with(')'));
        let inner = &rgb[4..rgb.len() - 1];
        let channels: Vec<u16> = inner
            .split(", ")
            .map(|p| p.parse::<u16>().unwrap())
            .collect();
        assert_eq!(channels.len(), 3);
        assert!(channels.iter().all(|c| *c <= 255));
    }

    #[test]
    fn generate_date_default_is_parseable() {
        let date = generate_date(&options()).unwrap();
        assert!(NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_ok());
    }

    #[test]
    fn generate_date_past_is_before_today() {
        let mut opts = options();
        opts.past = true;
        let date = NaiveDate::parse_from_str(&generate_date(&opts).unwrap(), "%Y-%m-%d").unwrap();
        assert!(date < Local::now().date_naive());
    }

    #[test]
    fn generate_date_future_is_after_today() {
        let mut opts = options();
        opts.future = true;
        let date = NaiveDate::parse_from_str(&generate_date(&opts).unwrap(), "%Y-%m-%d").unwrap();
        assert!(date > Local::now().date_naive());
    }

    #[test]
    fn generate_time_is_parseable() {
        let time = generate_time(&options()).unwrap();
        assert!(NaiveTime::parse_from_str(&time, "%H:%M:%S").is_ok());
    }

    #[test]
    fn generate_datetime_is_parseable() {
        let dt = generate_datetime(&options()).unwrap();
        assert!(NaiveDateTime::parse_from_str(&dt, "%Y-%m-%d %H:%M:%S").is_ok());
    }

    #[test]
    fn generate_timestamp_is_numeric() {
        let ts = generate_timestamp(&options()).unwrap();
        assert!(ts.parse::<i64>().is_ok());
    }

    #[test]
    fn generate_car_brand_is_non_empty() {
        assert!(!generate_car_brand(&options()).unwrap().is_empty());
    }

    fn seconds_of_day_now() -> u32 {
        Local::now().time().num_seconds_from_midnight().min(86_399)
    }

    fn parse_seconds(hhmmss: &str) -> u32 {
        NaiveTime::parse_from_str(hhmmss, "%H:%M:%S")
            .unwrap()
            .num_seconds_from_midnight()
    }

    #[test]
    fn generate_time_past_is_at_or_before_now() {
        let mut opts = options();
        opts.past = true;

        let before = seconds_of_day_now();
        let output = generate_time(&opts).unwrap();
        let after = seconds_of_day_now();

        let value = parse_seconds(&output);
        if after >= before {
            assert!(value <= after, "past time {value} should be <= now {after}");
        }
    }

    #[test]
    fn generate_time_future_is_at_or_after_now() {
        let mut opts = options();
        opts.future = true;

        let before = seconds_of_day_now();
        let output = generate_time(&opts).unwrap();
        let after = seconds_of_day_now();

        let value = parse_seconds(&output);
        if after >= before {
            assert!(
                value >= before,
                "future time {value} should be >= earlier now {before}"
            );
        }
    }

    #[test]
    fn generate_time_both_flags_produce_parseable_output() {
        let mut opts = options();
        opts.past = true;
        opts.future = true;

        let output = generate_time(&opts).unwrap();

        assert!(NaiveTime::parse_from_str(&output, "%H:%M:%S").is_ok());
    }
}
