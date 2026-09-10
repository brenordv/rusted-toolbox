use crate::models::MockConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_exit_helpers::exit_error;
use mock_data_utils::models::DataType;
use tracing::error;

/// Generate mock data for testing and development
///
/// Mock data generator CLI tool.
///  Generates various types of mock data including personal information,
///  internet data, random values, and commerce data.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about, after_long_help = get_help_text())]
pub struct CliArgs {
    /// Type of mock data to generate (e.g., person.first-name, internet.email)
    #[arg(value_name = "DATA_TYPE", index = 1, required = true)]
    pub data_type: String,

    /// Minimum value for generated data (e.g., for random integers)
    #[arg(short = 'm', long = "min", required = false)]
    pub min: Option<i32>,

    /// Maximum value for generated data (e.g., for random integers)
    #[arg(short = 'x', long = "max", required = false)]
    pub max: Option<i32>,

    /// Length of generated data (e.g., for strings or arrays)
    #[arg(short = 'n', long = "length", required = false)]
    pub length: Option<usize>,

    /// Precision for generated data (e.g., for floating-point numbers)
    #[arg(short = 'p', long = "precision", required = false)]
    pub precision: Option<u32>,

    /// Age for generated data (e.g., for birthday calculation)
    #[arg(short = 'a', long = "age", required = false)]
    pub age: Option<u32>,

    /// Generate date in the past
    #[arg(long = "past", required = false, conflicts_with = "future")]
    pub past: bool,

    /// Generate date in the future
    #[arg(
        short = 'f',
        long = "future",
        required = false,
        conflicts_with = "past"
    )]
    pub future: bool,

    /// Date range in years
    #[arg(short = 'r', long = "range", required = false, default_value_t = 50)]
    pub range: u32,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Get help text for available data types
fn get_help_text() -> &'static str {
    "Personal Information:
  person.first-name     - Generate a random first name
  person.last-name      - Generate a random last name
  person.full-name      - Generate a full name (first + last)
  person.email          - Generate a random email address
  person.phone          - Generate a phone number
  person.street         - Generate a street name
  person.city           - Generate a city name
  person.state          - Generate a state name
  person.country        - Generate a country name
  person.postal-code    - Generate postal/zip code
  person.address        - Generate a full address
  person.birthday       - Generate a birthday (with optional --age parameter)

Internet & Tech:
  internet.username     - Generate a username
  internet.password     - Generate a password (with --length option)
  internet.url          - Generate a URL
  internet.image-url    - Generate an image URL
  internet.file-url     - Generate a file URL

Random Data:
  random.date           - Generate a date (with --past, --future, or --range options)
  random.time           - Generate a time (with --past, --future options)
  random.datetime       - Generate a datetime (with --past, --future options)
  random.timestamp      - Generate a timestamp (with --past, --future options)
  random.color-hex      - Generate a hex color code
  random.color-rgb      - Generate RGB color values
  random.integer        - Generate an integer (with --min, --max options)
  random.float          - Generate a float (with --min, --max, --precision options)
  random.car-brand      - Generate a car brand name

Commerce:
  commerce.company      - Generate a company name
  commerce.product      - Generate a product name
  commerce.product-description - Generate a product description
  commerce.job-title    - Generate a job title
  commerce.industry     - Generate an industry name
  commerce.buzzword     - Generate a business buzzword"
}

/// Reject bounds where the minimum exceeds the maximum.
///
/// Only errors when both bounds are present; equal bounds are accepted.
pub(crate) fn validate_range(min: Option<i32>, max: Option<i32>) -> anyhow::Result<()> {
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            anyhow::bail!("Minimum value cannot be greater than maximum value");
        }
    }

    Ok(())
}

pub fn initialize() -> MockConfig {
    let args = CliArgs::parse();

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {}),
    );

    if let Err(err) = validate_range(args.min, args.max) {
        error!("{}", err);
        exit_error();
    }

    // Check if data type is provided
    if args.data_type.is_empty() {
        error!("Data type must be specified");
        exit_error();
    }

    let data_type = match DataType::from_command(args.data_type.as_str()) {
        Ok(data_type) => data_type,
        Err(err) => {
            error!("{}", err);
            exit_error();
        }
    };

    MockConfig::new(
        data_type,
        args.min,
        args.max,
        args.length,
        args.precision,
        args.age,
        args.past,
        args.future,
        Some(args.range),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Runs clap's self-checks, which catch definition errors such as two
    /// flags sharing the same short.
    #[test]
    fn cli_definition_passes_clap_debug_assertions() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn validate_range_rejects_min_greater_than_max() {
        let err = validate_range(Some(10), Some(5)).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Minimum value cannot be greater than maximum value"
        );
    }

    #[test]
    fn validate_range_accepts_min_equal_to_max() {
        assert!(validate_range(Some(7), Some(7)).is_ok());
    }

    #[test]
    fn validate_range_accepts_missing_bounds() {
        assert!(validate_range(None, None).is_ok());
        assert!(validate_range(Some(3), None).is_ok());
        assert!(validate_range(None, Some(3)).is_ok());
    }
}
