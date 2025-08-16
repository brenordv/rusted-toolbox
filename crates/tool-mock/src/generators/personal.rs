use crate::models::MockOptions;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use fake::faker::address::en::*;
use fake::faker::name::en::*;
use fake::faker::phone_number::en::*;
use fake::Fake;
use rand::Rng;

/// Generate a random first name
pub fn generate_first_name(_options: &MockOptions) -> Result<String> {
    Ok(FirstName().fake::<String>())
}

/// Generate a random last name
pub fn generate_last_name(_options: &MockOptions) -> Result<String> {
    Ok(LastName().fake::<String>())
}

/// Generate a full name (first + last)
pub fn generate_full_name(_options: &MockOptions) -> Result<String> {
    Ok(Name().fake::<String>())
}

/// Generate a random email address
pub fn generate_email(_options: &MockOptions) -> Result<String> {
    let first_name = FirstName().fake::<String>().to_lowercase();
    let last_name = LastName().fake::<String>().to_lowercase();
    let domains = [
        "gmail.com",
        "yahoo.com",
        "hotmail.com",
        "outlook.com",
        "example.com",
    ];
    let domain = domains[rand::thread_rng().gen_range(0..domains.len())];

    Ok(format!("{}.{}@{}", first_name, last_name, domain))
}

/// Generate a phone number
pub fn generate_phone(_options: &MockOptions) -> Result<String> {
    Ok(PhoneNumber().fake::<String>())
}

/// Generate a street name
pub fn generate_street(_options: &MockOptions) -> Result<String> {
    Ok(StreetName().fake::<String>())
}

/// Generate a city name
pub fn generate_city(_options: &MockOptions) -> Result<String> {
    Ok(CityName().fake::<String>())
}

/// Generate a state name
pub fn generate_state(_options: &MockOptions) -> Result<String> {
    Ok(StateName().fake::<String>())
}

/// Generate a country name
pub fn generate_country(_options: &MockOptions) -> Result<String> {
    Ok(CountryName().fake::<String>())
}

/// Generate a postal/zip code
pub fn generate_postal_code(_options: &MockOptions) -> Result<String> {
    Ok(PostCode().fake::<String>())
}

/// Generate a full address
pub fn generate_address(_options: &MockOptions) -> Result<String> {
    let street_number: u32 = (1..9999).fake();
    let street_name = StreetName().fake::<String>();
    let city = CityName().fake::<String>();
    let state = StateAbbr().fake::<String>();
    let zip = PostCode().fake::<String>();

    Ok(format!(
        "{} {}, {}, {} {}",
        street_number, street_name, city, state, zip
    ))
}

/// Generate a birthday
pub fn generate_birthday(options: &MockOptions) -> Result<String> {
    let today = Local::now().date_naive();

    let age = options
        .age
        .unwrap_or_else(|| rand::thread_rng().gen_range(18..80));

    // Calculate birth year
    let birth_year = today.year() - age as i32;

    // Generate random month and day
    let month = rand::thread_rng().gen_range(1..=12);
    let day = match month {
        2 => {
            // Handle February and leap years
            let is_leap = (birth_year % 4 == 0 && birth_year % 100 != 0) || (birth_year % 400 == 0);
            if is_leap {
                rand::thread_rng().gen_range(1..=29)
            } else {
                rand::thread_rng().gen_range(1..=28)
            }
        }
        4 | 6 | 9 | 11 => rand::thread_rng().gen_range(1..=30),
        _ => rand::thread_rng().gen_range(1..=31),
    };

    let birthday = NaiveDate::from_ymd_opt(birth_year, month, day)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(birth_year, 1, 1).unwrap());

    Ok(birthday.format("%Y-%m-%d").to_string())
}
