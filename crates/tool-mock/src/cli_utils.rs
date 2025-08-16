use crate::models::MockArgs;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;

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

Commerce:
  commerce.company      - Generate a company name
  commerce.product      - Generate a product name
  commerce.product-description - Generate a product description
  commerce.job-title    - Generate a job title
  commerce.industry     - Generate an industry name
  commerce.buzzword     - Generate a business buzzword"
}

/// Parses command-line arguments for mock data generator application.
///
/// Supports various data types with flexible options for customization.
/// Generates one piece of mock data per execution.
///
/// # Errors
/// Returns error if argument parsing fails
///
/// # Supported Commands
/// - Personal: person.first-name, person.email, person.address, etc.
/// - Internet: internet.username, internet.password, internet.url, etc.
/// - Random: random.date, random.integer, random.color-hex, etc.
/// - Commerce: commerce.company, commerce.product, commerce.job-title, etc.
///
/// # Global Options
/// - `--locale <LOCALE>`: Set locale for region-specific data (default: en_US)
///
/// # Data-specific Options
/// - `--min <NUMBER>`: Minimum value (for numbers)
/// - `--max <NUMBER>`: Maximum value (for numbers)
/// - `--length <NUMBER>`: Length specification (for passwords, etc.)
/// - `--precision <NUMBER>`: Decimal precision (for floats)
/// - `--age <NUMBER>`: Age for birthday calculation
/// - `--past`: Generate past dates/times
/// - `--future`: Generate future dates/times
/// - `--range <YEARS>`: Date range in years (default: 50)
///
/// # Metadata
///
/// - Name: `MOCK_APP_NAME` (constant).
/// - Version: `MOCK_VERSION` (constant).
/// - Description: Generates mock data for testing and development purposes.
///
/// # Dependencies
/// This function uses the `clap` crate for defining and parsing command-line arguments.
///
/// # Examples
/// ```bash
/// mock person.first-name
/// mock person.email
/// mock random.integer --min 1 --max 100
/// mock internet.password --length 12
/// mock random.date --past
/// ```
pub fn get_cli_arguments() -> MockArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Generate mock data for testing and development",
            "Mock data generator CLI tool.\n\n\
            Generates various types of mock data including personal information, \
            internet data, random values, and commerce data.\n\n\
            USAGE:\n    \
            mock [DATA_TYPE] [OPTIONS]\n\n\
            EXAMPLES:\n    \
            mock person.first-name\n    \
            mock person.email\n    \
            mock random.integer --min 1 --max 100\n    \
            mock internet.password --length 12\n    \
            mock random.date --past\n\n\
            AVAILABLE DATA TYPES:\n\n",
        )
        .after_help(get_help_text())
        .arg(
            Arg::new("data_type")
                .help("Type of mock data to generate (e.g., person.first-name, internet.email)")
                .value_name("DATA_TYPE")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::new("locale")
                .long("locale")
                .value_name("LOCALE")
                .help("Set locale for region-specific data")
                .default_value("en_US"),
        )
        .arg(
            Arg::new("min")
                .long("min")
                .value_name("NUMBER")
                .help("Minimum value (for numbers)"),
        )
        .arg(
            Arg::new("max")
                .long("max")
                .value_name("NUMBER")
                .help("Maximum value (for numbers)"),
        )
        .arg(
            Arg::new("length")
                .long("length")
                .value_name("NUMBER")
                .help("Length specification (for passwords, strings, etc.)"),
        )
        .arg(
            Arg::new("precision")
                .long("precision")
                .value_name("NUMBER")
                .help("Decimal precision (for floats)"),
        )
        .arg(
            Arg::new("age")
                .long("age")
                .value_name("NUMBER")
                .help("Age for birthday calculation"),
        )
        .arg(
            Arg::new("past")
                .long("past")
                .action(clap::ArgAction::SetTrue)
                .help("Generate past dates/times"),
        )
        .arg(
            Arg::new("future")
                .long("future")
                .action(clap::ArgAction::SetTrue)
                .help("Generate future dates/times"),
        )
        .arg(
            Arg::new("range")
                .long("range")
                .value_name("YEARS")
                .help("Date range in years (default: 50)"),
        )
        .get_matches();

    MockArgs::parse(&matches)
}
