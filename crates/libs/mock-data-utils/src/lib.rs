//! Mock data generators for `tool-mock`.
//!
//! [`generate_mock_data`] dispatches a [`models::MockOptions`] to one of the
//! generators grouped under [`generators`] (personal, internet, random,
//! commerce). Input is validated first, so an out-of-range option (`min > max`,
//! `range == 0`) returns an error instead of panicking on an empty random range.

use crate::generators::*;
use crate::models::{DataType, MockOptions};
use anyhow::Result;

pub mod generators;
pub mod models;

/// Generate mock data based on the specified data type and options.
///
/// # Errors
/// Returns an error when the options are invalid for the requested type: `min`
/// greater than `max` for integers and floats, or a `range` of zero for the
/// date, datetime, and timestamp types.
pub fn generate_mock_data(options: &MockOptions) -> Result<String> {
    validate(options, &options.data_type)?;

    match options.data_type {
        // Personal Information
        DataType::FirstName => generate_first_name(options),
        DataType::LastName => generate_last_name(options),
        DataType::FullName => generate_full_name(options),
        DataType::Email => generate_email(options),
        DataType::Phone => generate_phone(options),
        DataType::Street => generate_street(options),
        DataType::City => generate_city(options),
        DataType::State => generate_state(options),
        DataType::Country => generate_country(options),
        DataType::PostalCode => generate_postal_code(options),
        DataType::Address => generate_address(options),
        DataType::Birthday => generate_birthday(options),

        // Internet & Tech
        DataType::Username => generate_username(options),
        DataType::Password => generate_password(options),
        DataType::Url => generate_url(options),
        DataType::ImageUrl => generate_image_url(options),
        DataType::FileUrl => generate_file_url(options),

        // Random Data
        DataType::Date => generate_date(options),
        DataType::Time => generate_time(options),
        DataType::DateTime => generate_datetime(options),
        DataType::Timestamp => generate_timestamp(options),
        DataType::ColorHex => generate_color_hex(options),
        DataType::ColorRgb => generate_color_rgb(options),
        DataType::Integer => generate_integer(options),
        DataType::Float => generate_float(options),
        DataType::CarBrand => generate_car_brand(options),

        // Commerce
        DataType::Company => generate_company(options),
        DataType::Product => generate_product(options),
        DataType::ProductDescription => generate_product_description(options),
        DataType::JobTitle => generate_job_title(options),
        DataType::Industry => generate_industry(options),
        DataType::Buzzword => generate_buzzword(options),
    }
}

/// Rejects option values that would otherwise reach an empty random range and
/// panic: `min > max` for numeric types, and a zero `range` for the date-based
/// types that offset by it.
fn validate(options: &MockOptions, data_type: &DataType) -> Result<()> {
    match data_type {
        DataType::Integer | DataType::Float => {
            let min = options.min.unwrap_or(0);
            let max = options.max.unwrap_or(100);
            if min > max {
                anyhow::bail!("min ({min}) must be <= max ({max})");
            }
        }
        DataType::Date | DataType::DateTime | DataType::Timestamp if options.range == Some(0) => {
            anyhow::bail!("range must be >= 1");
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options_for(data_type: DataType) -> MockOptions {
        MockOptions {
            data_type,
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

    // Update this list when adding a DataType variant. The compiler will not
    // catch a missing entry; the from_command round-trip below partially does.
    const ALL_VARIANTS: [DataType; 32] = [
        DataType::FirstName,
        DataType::LastName,
        DataType::FullName,
        DataType::Email,
        DataType::Phone,
        DataType::Street,
        DataType::City,
        DataType::State,
        DataType::Country,
        DataType::PostalCode,
        DataType::Address,
        DataType::Birthday,
        DataType::Username,
        DataType::Password,
        DataType::Url,
        DataType::ImageUrl,
        DataType::FileUrl,
        DataType::Date,
        DataType::Time,
        DataType::DateTime,
        DataType::Timestamp,
        DataType::ColorHex,
        DataType::ColorRgb,
        DataType::Integer,
        DataType::Float,
        DataType::CarBrand,
        DataType::Company,
        DataType::Product,
        DataType::ProductDescription,
        DataType::JobTitle,
        DataType::Industry,
        DataType::Buzzword,
    ];

    // Extracts the dotted command tokens (e.g. "person.first-name") from the
    // help text produced by `all_commands`.
    fn advertised_commands() -> Vec<String> {
        DataType::all_commands()
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|token| token.contains('.'))
            .map(|token| token.to_string())
            .collect()
    }

    #[test]
    fn generate_mock_data_produces_non_empty_for_every_variant() {
        for variant in ALL_VARIANTS {
            let output = generate_mock_data(&options_for(variant.clone())).unwrap();
            assert!(!output.is_empty(), "empty output for {variant:?}");
        }
    }

    #[test]
    fn validate_integer_min_greater_than_max_errors() {
        let mut opts = options_for(DataType::Integer);
        opts.min = Some(5);
        opts.max = Some(1);

        let error = generate_mock_data(&opts).unwrap_err();

        assert!(format!("{error:#}").contains("min"));
    }

    #[test]
    fn validate_integer_min_equals_max_is_ok() {
        let mut opts = options_for(DataType::Integer);
        opts.min = Some(7);
        opts.max = Some(7);

        assert_eq!(generate_mock_data(&opts).unwrap(), "7");
    }

    #[test]
    fn validate_float_min_greater_than_max_errors() {
        let mut opts = options_for(DataType::Float);
        opts.min = Some(5);
        opts.max = Some(1);

        let error = generate_mock_data(&opts).unwrap_err();

        assert!(format!("{error:#}").contains("min"));
    }

    #[test]
    fn validate_date_range_zero_errors() {
        let mut opts = options_for(DataType::Date);
        opts.range = Some(0);

        let error = generate_mock_data(&opts).unwrap_err();

        assert!(format!("{error:#}").contains("range"));
    }

    #[test]
    fn validate_timestamp_range_zero_errors() {
        let mut opts = options_for(DataType::Timestamp);
        opts.range = Some(0);

        let error = generate_mock_data(&opts).unwrap_err();

        assert!(format!("{error:#}").contains("range"));
    }

    #[test]
    fn from_command_round_trips_every_advertised_command() {
        for command in advertised_commands() {
            assert!(
                DataType::from_command(&command).is_ok(),
                "command did not parse: {command}"
            );
        }
    }

    #[test]
    fn from_command_rejects_unknown_command_naming_it() {
        let error = DataType::from_command("bogus.command").unwrap_err();

        assert!(error.contains("bogus.command"));
    }

    #[test]
    fn all_commands_still_advertises_past_for_random_time() {
        let line = DataType::all_commands()
            .lines()
            .find(|line| line.contains("random.time"))
            .expect("random.time is listed")
            .to_string();

        assert!(line.contains("--past"));
    }
}
