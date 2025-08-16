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
