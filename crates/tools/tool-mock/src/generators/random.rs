use crate::models::MockOptions;
use anyhow::Result;
use chrono::{Duration, Local, NaiveTime, Utc};
use rand::Rng;

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

/// Generate a random time
pub fn generate_time(options: &MockOptions) -> Result<String> {
    let now = Local::now().time();

    let time = if options.past || options.future {
        // Generate random time within the day
        let hour = rand::rng().random_range(0..24);
        let minute = rand::rng().random_range(0..60);
        let second = rand::rng().random_range(0..60);
        NaiveTime::from_hms_opt(hour, minute, second).unwrap_or(now)
    } else {
        // Generate completely random time
        let hour = rand::rng().random_range(0..24);
        let minute = rand::rng().random_range(0..60);
        let second = rand::rng().random_range(0..60);
        NaiveTime::from_hms_opt(hour, minute, second).unwrap_or(now)
    };

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

    Ok(format!("#{:02x}{:02x}{:02x}", r, g, b))
}

/// Generate random RGB color values
pub fn generate_color_rgb(_options: &MockOptions) -> Result<String> {
    let r = rand::rng().random_range(0..256);
    let g = rand::rng().random_range(0..256);
    let b = rand::rng().random_range(0..256);

    Ok(format!("rgb({}, {}, {})", r, g, b))
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
}
