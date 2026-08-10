use clap::ArgMatches;

/// Available data types within each category
#[derive(Debug, Clone)]
pub enum DataType {
    // Personal Information
    FirstName,
    LastName,
    FullName,
    Email,
    Phone,
    Street,
    City,
    State,
    Country,
    PostalCode,
    Address,
    Birthday,

    // Internet & Tech
    Username,
    Password,
    Url,
    ImageUrl,
    FileUrl,

    // Random Data
    Date,
    Time,
    DateTime,
    Timestamp,
    ColorHex,
    ColorRgb,
    Integer,
    Float,
    CarBrand,

    // Commerce
    Company,
    Product,
    ProductDescription,
    JobTitle,
    Industry,
    Buzzword,
}

impl DataType {
    /// Parse data type from string command (e.g., "person.first-name")
    pub fn from_command(command: &str) -> Result<Self, String> {
        match command {
            // Personal Information
            "person.first-name" => Ok(DataType::FirstName),
            "person.last-name" => Ok(DataType::LastName),
            "person.full-name" => Ok(DataType::FullName),
            "person.email" => Ok(DataType::Email),
            "person.phone" => Ok(DataType::Phone),
            "person.street" => Ok(DataType::Street),
            "person.city" => Ok(DataType::City),
            "person.state" => Ok(DataType::State),
            "person.country" => Ok(DataType::Country),
            "person.postal-code" => Ok(DataType::PostalCode),
            "person.address" => Ok(DataType::Address),
            "person.birthday" => Ok(DataType::Birthday),

            // Internet & Tech
            "internet.username" => Ok(DataType::Username),
            "internet.password" => Ok(DataType::Password),
            "internet.url" => Ok(DataType::Url),
            "internet.image-url" => Ok(DataType::ImageUrl),
            "internet.file-url" => Ok(DataType::FileUrl),

            // Random Data
            "random.date" => Ok(DataType::Date),
            "random.time" => Ok(DataType::Time),
            "random.datetime" => Ok(DataType::DateTime),
            "random.timestamp" => Ok(DataType::Timestamp),
            "random.color-hex" => Ok(DataType::ColorHex),
            "random.color-rgb" => Ok(DataType::ColorRgb),
            "random.integer" => Ok(DataType::Integer),
            "random.float" => Ok(DataType::Float),
            "random.car-brand" => Ok(DataType::CarBrand),

            // Commerce
            "commerce.company" => Ok(DataType::Company),
            "commerce.product" => Ok(DataType::Product),
            "commerce.product-description" => Ok(DataType::ProductDescription),
            "commerce.job-title" => Ok(DataType::JobTitle),
            "commerce.industry" => Ok(DataType::Industry),
            "commerce.buzzword" => Ok(DataType::Buzzword),

            _ => Err(format!("Unknown data type: {}", command)),
        }
    }

    /// Get all available commands as a formatted string
    pub fn all_commands() -> String {
        vec![
            "Personal Information:",
            "  person.first-name     - Generate a random first name",
            "  person.last-name      - Generate a random last name",
            "  person.full-name      - Generate a full name (first + last)",
            "  person.email          - Generate a random email address",
            "  person.phone          - Generate a phone number",
            "  person.street         - Generate a street name",
            "  person.city           - Generate a city name",
            "  person.state          - Generate a state name",
            "  person.country        - Generate a country name",
            "  person.postal-code    - Generate postal/zip code",
            "  person.address        - Generate a full address",
            "  person.birthday       - Generate a birthday (with optional --age parameter)",
            "",
            "Internet & Tech:",
            "  internet.username     - Generate a username",
            "  internet.password     - Generate a password (with --length option)",
            "  internet.url          - Generate a URL",
            "  internet.image-url    - Generate an image URL",
            "  internet.file-url     - Generate a file URL",
            "",
            "Random Data:",
            "  random.date           - Generate a date (with --past, --future, or --range options)",
            "  random.time           - Generate a time (with --past, --future options)",
            "  random.datetime       - Generate a datetime (with --past, --future options)",
            "  random.timestamp      - Generate a timestamp (with --past, --future options)",
            "  random.color-hex      - Generate a hex color code",
            "  random.color-rgb      - Generate RGB color values",
            "  random.integer        - Generate an integer (with --min, --max options)",
            "  random.float          - Generate a float (with --min, --max, --precision options)",
            "  random.car-brand      - Generate a car brand name",
            "",
            "Commerce:",
            "  commerce.company      - Generate a company name",
            "  commerce.product      - Generate a product name",
            "  commerce.product-description - Generate a product description",
            "  commerce.job-title    - Generate a job title",
            "  commerce.industry     - Generate an industry name",
            "  commerce.buzzword     - Generate a business buzzword",
        ]
        .join("\n")
    }
}

/// Command-line arguments for mock data generator
#[derive(Debug)]
pub struct MockArgs {
    pub data_type: Option<String>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub length: Option<usize>,
    pub precision: Option<u32>,
    pub age: Option<u32>,
    pub past: bool,
    pub future: bool,
    pub range: Option<u32>,
}

impl MockArgs {
    /// Parse command-line arguments from clap matches
    pub fn parse(args: &ArgMatches) -> Self {
        MockArgs {
            data_type: args.get_one::<String>("data_type").cloned(),
            min: args.get_one::<String>("min").and_then(|s| s.parse().ok()),
            max: args.get_one::<String>("max").and_then(|s| s.parse().ok()),
            length: args
                .get_one::<String>("length")
                .and_then(|s| s.parse().ok()),
            precision: args
                .get_one::<String>("precision")
                .and_then(|s| s.parse().ok()),
            age: args.get_one::<String>("age").and_then(|s| s.parse().ok()),
            past: args.get_flag("past"),
            future: args.get_flag("future"),
            range: args.get_one::<String>("range").and_then(|s| s.parse().ok()),
        }
    }

    /// Validate argument combinations
    pub fn validate(&self) -> Result<(), String> {
        // Check for conflicting time options
        if self.past && self.future {
            return Err("Cannot specify both --past and --future options".to_string());
        }

        // Check min/max ranges
        if let (Some(min), Some(max)) = (self.min, self.max) {
            if min > max {
                return Err("Minimum value cannot be greater than maximum value".to_string());
            }
        }

        // Check if data type is provided
        if self.data_type.is_none() {
            return Err("Data type must be specified".to_string());
        }

        Ok(())
    }
}

/// Configuration options for mock data generation
#[derive(Debug)]
pub struct MockOptions {
    pub data_type: DataType,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub length: Option<usize>,
    pub precision: Option<u32>,
    pub age: Option<u32>,
    pub past: bool,
    pub future: bool,
    pub range: Option<u32>,
}

impl MockOptions {
    /// Create MockOptions from command-line arguments
    pub fn from_args(args: &MockArgs) -> Result<Self, String> {
        args.validate()?;

        let data_type_str = args.data_type.as_ref().unwrap();
        let data_type = DataType::from_command(data_type_str)?;

        Ok(MockOptions {
            data_type,
            min: args.min,
            max: args.max,
            length: args.length,
            precision: args.precision,
            age: args.age,
            past: args.past,
            future: args.future,
            range: args.range,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, ArgAction, Command};

    fn matches_from(args: &[&str]) -> ArgMatches {
        Command::new("mock")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .arg(Arg::new("data_type").index(1))
            .arg(Arg::new("min").long("min"))
            .arg(Arg::new("max").long("max"))
            .arg(Arg::new("length").long("length"))
            .arg(Arg::new("precision").long("precision"))
            .arg(Arg::new("age").long("age"))
            .arg(Arg::new("past").long("past").action(ArgAction::SetTrue))
            .arg(Arg::new("future").long("future").action(ArgAction::SetTrue))
            .arg(Arg::new("range").long("range"))
            .try_get_matches_from(args)
            .expect("valid args")
    }

    fn base_args(data_type: &str) -> MockArgs {
        MockArgs {
            data_type: Some(data_type.to_string()),
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
    fn from_command_maps_known_person_command() {
        assert!(matches!(
            DataType::from_command("person.first-name").unwrap(),
            DataType::FirstName
        ));
    }

    #[test]
    fn from_command_maps_known_commerce_command() {
        assert!(matches!(
            DataType::from_command("commerce.buzzword").unwrap(),
            DataType::Buzzword
        ));
    }

    #[test]
    fn from_command_unknown_returns_error() {
        let result = DataType::from_command("person.unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown data type"));
    }

    #[test]
    fn all_commands_lists_representative_entries() {
        let text = DataType::all_commands();
        assert!(text.contains("person.first-name"));
        assert!(text.contains("internet.username"));
        assert!(text.contains("random.integer"));
        assert!(text.contains("commerce.company"));
    }

    #[test]
    fn parse_reads_flags_and_numbers() {
        let matches = matches_from(&[
            "mock",
            "random.integer",
            "--min",
            "5",
            "--max",
            "10",
            "--past",
        ]);
        let args = MockArgs::parse(&matches);
        assert_eq!(args.data_type.as_deref(), Some("random.integer"));
        assert_eq!(args.min, Some(5));
        assert_eq!(args.max, Some(10));
        assert!(args.past);
        assert!(!args.future);
    }

    #[test]
    fn parse_ignores_unparseable_numbers() {
        let matches = matches_from(&["mock", "random.integer", "--min", "notanumber"]);
        let args = MockArgs::parse(&matches);
        assert_eq!(args.min, None);
    }

    #[test]
    fn validate_ok_for_minimal_args() {
        assert!(base_args("person.email").validate().is_ok());
    }

    #[test]
    fn validate_rejects_past_and_future_together() {
        let mut args = base_args("random.date");
        args.past = true;
        args.future = true;
        let err = args.validate().unwrap_err();
        assert!(err.contains("both --past and --future"));
    }

    #[test]
    fn validate_rejects_min_greater_than_max() {
        let mut args = base_args("random.integer");
        args.min = Some(10);
        args.max = Some(5);
        assert!(args.validate().is_err());
    }

    #[test]
    fn validate_requires_data_type() {
        let mut args = base_args("ignored");
        args.data_type = None;
        let err = args.validate().unwrap_err();
        assert!(err.contains("Data type must be specified"));
    }

    #[test]
    fn from_args_builds_options_for_valid_input() {
        let options = MockOptions::from_args(&base_args("internet.password")).unwrap();
        assert!(matches!(options.data_type, DataType::Password));
    }

    #[test]
    fn from_args_propagates_validation_error() {
        let mut args = base_args("random.integer");
        args.min = Some(10);
        args.max = Some(1);
        assert!(MockOptions::from_args(&args).is_err());
    }

    #[test]
    fn from_args_rejects_unknown_data_type() {
        assert!(MockOptions::from_args(&base_args("nope.nope")).is_err());
    }
}
