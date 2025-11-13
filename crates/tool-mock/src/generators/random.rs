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
