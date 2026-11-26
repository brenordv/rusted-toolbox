use crate::models::MockOptions;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use fake::Fake;
use rand::RngExt;

/// Generate a random first name
pub fn generate_first_name(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, name::FirstName))
}

/// Generate a random last name
pub fn generate_last_name(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, name::LastName))
}

/// Generate a full name (first + last)
pub fn generate_full_name(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, name::Name))
}

/// Generate a random email address
pub fn generate_email(options: &MockOptions) -> Result<String> {
    let first_name = localized!(options.locale, name::FirstName).to_lowercase();
    let last_name = localized!(options.locale, name::LastName).to_lowercase();
    let domains = [
        "gmail.com",
        "yahoo.com",
        "hotmail.com",
        "outlook.com",
        "example.com",
    ];
    let domain = domains[rand::rng().random_range(0..domains.len())];

    Ok(format!("{first_name}.{last_name}@{domain}"))
}

/// Generate a phone number
pub fn generate_phone(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, phone_number::PhoneNumber))
}

/// Generate a street name
pub fn generate_street(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, address::StreetName))
}

/// Generate a city name
pub fn generate_city(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, address::CityName))
}

/// Generate a state name
pub fn generate_state(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, address::StateName))
}

/// Generate a country name
pub fn generate_country(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, address::CountryName))
}

/// Generate a postal/zip code
pub fn generate_postal_code(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, address::PostCode))
}

/// Generate a full address
pub fn generate_address(options: &MockOptions) -> Result<String> {
    let street_number: u32 = (1..9999).fake();
    let street_name = localized!(options.locale, address::StreetName);
    let city = localized!(options.locale, address::CityName);
    let state = localized!(options.locale, address::StateAbbr);
    let zip = localized!(options.locale, address::PostCode);

    Ok(format!(
        "{street_number} {street_name}, {city}, {state} {zip}"
    ))
}

/// Generate a birthday
pub fn generate_birthday(options: &MockOptions) -> Result<String> {
    let today = Local::now().date_naive();

    let age = options
        .age
        .unwrap_or_else(|| rand::rng().random_range(18..80));

    // Calculate birth year
    let birth_year = today.year() - age as i32;

    // Generate random month and day
    let month = rand::rng().random_range(1..=12);
    let day = match month {
        2 => {
            // Handle February and leap years
            let is_leap = (birth_year % 4 == 0 && birth_year % 100 != 0) || (birth_year % 400 == 0);
            if is_leap {
                rand::rng().random_range(1..=29)
            } else {
                rand::rng().random_range(1..=28)
            }
        }
        4 | 6 | 9 | 11 => rand::rng().random_range(1..=30),
        _ => rand::rng().random_range(1..=31),
    };

    let birthday = NaiveDate::from_ymd_opt(birth_year, month, day)
        .or_else(|| NaiveDate::from_ymd_opt(birth_year, 1, 1))
        .ok_or_else(|| {
            anyhow::anyhow!("birth year {birth_year} is outside the representable date range")
        })?;

    Ok(birthday.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DataType, Locale};
    use chrono::Datelike;
    use rstest::rstest;

    fn options() -> MockOptions {
        MockOptions {
            data_type: DataType::Email,
            min: None,
            max: None,
            length: None,
            precision: None,
            age: None,
            past: false,
            future: false,
            range: None,
            locale: Locale::En,
        }
    }

    #[test]
    fn generate_email_is_lowercase_with_known_domain() {
        let email = generate_email(&options()).unwrap();
        assert!(email.contains('@'));
        assert_eq!(email, email.to_lowercase());
        let domain = email.split('@').nth(1).unwrap();
        let known = [
            "gmail.com",
            "yahoo.com",
            "hotmail.com",
            "outlook.com",
            "example.com",
        ];
        assert!(known.contains(&domain));
    }

    #[test]
    fn generate_birthday_matches_requested_age() {
        let mut opts = options();
        opts.age = Some(30);
        let year_before = Local::now().year();
        let birthday = generate_birthday(&opts).unwrap();
        let year_after = Local::now().year();
        let parsed = NaiveDate::parse_from_str(&birthday, "%Y-%m-%d").unwrap();
        assert!(parsed.year() == year_before - 30 || parsed.year() == year_after - 30);
    }

    #[test]
    fn generate_address_has_comma_separated_parts() {
        let address = generate_address(&options()).unwrap();
        assert!(address.matches(',').count() >= 2);
    }

    #[rstest]
    #[case::first_name(generate_first_name)]
    #[case::last_name(generate_last_name)]
    #[case::full_name(generate_full_name)]
    #[case::phone(generate_phone)]
    #[case::street(generate_street)]
    #[case::city(generate_city)]
    #[case::state(generate_state)]
    #[case::country(generate_country)]
    #[case::postal_code(generate_postal_code)]
    fn generator_output_is_non_empty(#[case] generator: fn(&MockOptions) -> Result<String>) {
        assert!(!generator(&options()).unwrap().is_empty());
    }
}
