use crate::generators::*;
use crate::models::{DataType, MockOptions};
use anyhow::Result;

/// Generate mock data based on the specified data type and options
pub fn generate_mock_data(options: &MockOptions) -> Result<String> {
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

        // Commerce
        DataType::Company => generate_company(options),
        DataType::Product => generate_product(options),
        DataType::ProductDescription => generate_product_description(options),
        DataType::JobTitle => generate_job_title(options),
        DataType::Industry => generate_industry(options),
        DataType::Buzzword => generate_buzzword(options),
    }
}
