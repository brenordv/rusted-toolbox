/// Locale for the fake-backed generators.
///
/// Selects which of the `fake` crate's locale data sets the generators draw
/// from. Data types whose [`DataType::is_locale_aware`] is `false` ignore it
/// entirely, and `fake` itself falls back to its English data for any
/// category a locale does not localize, so every locale is valid for every
/// data type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    En,
    ArSa,
    CyGb,
    DeDe,
    FaIr,
    FrFr,
    ItIt,
    JaJp,
    NlNl,
    PtBr,
    PtPt,
    TrTr,
    ZhCn,
    ZhTw,
}

impl Locale {
    /// Every locale, in canonical order. Update alongside a new enum variant;
    /// the exhaustive match in [`Locale::code`] is what the compiler checks.
    pub const ALL: [Locale; 14] = [
        Locale::En,
        Locale::ArSa,
        Locale::CyGb,
        Locale::DeDe,
        Locale::FaIr,
        Locale::FrFr,
        Locale::ItIt,
        Locale::JaJp,
        Locale::NlNl,
        Locale::PtBr,
        Locale::PtPt,
        Locale::TrTr,
        Locale::ZhCn,
        Locale::ZhTw,
    ];

    /// Parses a locale code, case-insensitively, with `-` and `_` accepted
    /// interchangeably as the separator (`pt-br`, `PT_BR`, `pt_Br`).
    ///
    /// # Errors
    /// Returns an error naming the rejected code and listing the valid ones.
    pub fn from_code(code: &str) -> Result<Self, String> {
        let normalized = code.trim().to_ascii_lowercase().replace('_', "-");
        Self::ALL
            .iter()
            .find(|locale| locale.code() == normalized)
            .copied()
            .ok_or_else(|| {
                format!(
                    "Unknown locale '{code}'. Valid codes: {}",
                    Self::all_codes().join(", ")
                )
            })
    }

    /// The canonical dashed lowercase code (`"pt-br"`).
    pub fn code(&self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::ArSa => "ar-sa",
            Locale::CyGb => "cy-gb",
            Locale::DeDe => "de-de",
            Locale::FaIr => "fa-ir",
            Locale::FrFr => "fr-fr",
            Locale::ItIt => "it-it",
            Locale::JaJp => "ja-jp",
            Locale::NlNl => "nl-nl",
            Locale::PtBr => "pt-br",
            Locale::PtPt => "pt-pt",
            Locale::TrTr => "tr-tr",
            Locale::ZhCn => "zh-cn",
            Locale::ZhTw => "zh-tw",
        }
    }

    /// Every valid code, in the order of [`Locale::ALL`], for help text.
    pub fn all_codes() -> Vec<&'static str> {
        Self::ALL.iter().map(Locale::code).collect()
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// Configuration options for mock data generation.
#[derive(Debug)]
pub struct MockOptions {
    /// Which kind of data to generate.
    pub data_type: DataType,
    /// Inclusive lower bound; honored by `random.integer` and `random.float`.
    pub min: Option<i32>,
    /// Inclusive upper bound; honored by `random.integer` and `random.float`.
    pub max: Option<i32>,
    /// Output length; honored by `internet.password` and other length-bounded
    /// generators such as `commerce.product-description`. Zero is honored
    /// literally and yields an empty string.
    pub length: Option<usize>,
    /// Number of decimal places; honored by `random.float`. Values above
    /// `MAX_PRECISION` (100) are rejected by validation.
    pub precision: Option<u32>,
    /// Target age in years; honored by `person.birthday`. Values above
    /// `MAX_YEAR_OFFSET` (100,000) are rejected by validation to keep the
    /// derived birth year representable.
    pub age: Option<u32>,
    /// Restrict output to the past; honored by `random.date`, `random.time`,
    /// `random.datetime`, and `random.timestamp`. When both `past` and `future`
    /// are set, `past` wins.
    pub past: bool,
    /// Restrict output to the future; honored by `random.date`, `random.time`,
    /// `random.datetime`, and `random.timestamp`. When both `past` and `future`
    /// are set, `past` wins.
    pub future: bool,
    /// Half-width in years of the sampling window; honored by `random.date`,
    /// `random.datetime`, and `random.timestamp`. Must be at least 1 and at
    /// most `MAX_YEAR_OFFSET` (100,000), so the sampled offset stays
    /// representable.
    pub range: Option<u32>,
    /// Locale for the fake-backed generators; honored exactly by the data
    /// types whose [`DataType::is_locale_aware`] is `true` (the `person.*`
    /// types except birthday, `internet.username`, `commerce.company`,
    /// `commerce.job-title`, `commerce.industry`). The hand-rolled English
    /// lists (product, buzzword, ...), the structural generators (password,
    /// URLs), and the `random.*` types ignore it.
    pub locale: Locale,
}

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

            _ => Err(format!("Unknown data type: {command}")),
        }
    }

    /// Reports whether this data type's generator draws from the `fake`
    /// crate's locale-specific data and therefore honors
    /// [`MockOptions::locale`].
    pub fn is_locale_aware(&self) -> bool {
        matches!(
            self,
            DataType::FirstName
                | DataType::LastName
                | DataType::FullName
                | DataType::Email
                | DataType::Phone
                | DataType::Street
                | DataType::City
                | DataType::State
                | DataType::Country
                | DataType::PostalCode
                | DataType::Address
                | DataType::Username
                | DataType::Company
                | DataType::JobTitle
                | DataType::Industry
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_default_is_en() {
        assert_eq!(Locale::default(), Locale::En);
    }

    #[test]
    fn locale_codes_round_trip_through_from_code() {
        for locale in Locale::ALL {
            assert_eq!(Locale::from_code(locale.code()), Ok(locale));
        }
    }

    #[test]
    fn locale_from_code_accepts_underscores_and_mixed_case() {
        assert_eq!(Locale::from_code("PT_BR"), Ok(Locale::PtBr));
        assert_eq!(Locale::from_code("pt_Br"), Ok(Locale::PtBr));
        assert_eq!(Locale::from_code("Zh-CN"), Ok(Locale::ZhCn));
        assert_eq!(Locale::from_code(" en "), Ok(Locale::En));
    }

    #[test]
    fn locale_from_code_rejects_unknown_naming_it() {
        let error = Locale::from_code("xx-yy").unwrap_err();

        assert!(error.contains("xx-yy"));
        assert!(error.contains("pt-br"));
    }

    #[test]
    fn locale_display_matches_code() {
        assert_eq!(Locale::JaJp.to_string(), "ja-jp");
        assert_eq!(Locale::En.to_string(), "en");
    }

    #[test]
    fn all_codes_lists_every_locale_once() {
        let codes = Locale::all_codes();

        assert_eq!(codes.len(), Locale::ALL.len());
        assert_eq!(codes.first(), Some(&"en"));
    }

    // The exhaustive match stops compiling when a variant is added, which is
    // the prompt to extend `ALL` (and this map) alongside it; the assertions
    // then pin that every variant sits in `ALL` exactly where the map says.
    #[test]
    fn all_is_exhaustive_over_the_enum() {
        fn position(locale: Locale) -> usize {
            match locale {
                Locale::En => 0,
                Locale::ArSa => 1,
                Locale::CyGb => 2,
                Locale::DeDe => 3,
                Locale::FaIr => 4,
                Locale::FrFr => 5,
                Locale::ItIt => 6,
                Locale::JaJp => 7,
                Locale::NlNl => 8,
                Locale::PtBr => 9,
                Locale::PtPt => 10,
                Locale::TrTr => 11,
                Locale::ZhCn => 12,
                Locale::ZhTw => 13,
            }
        }

        assert_eq!(Locale::ALL.len(), 14);
        for (index, locale) in Locale::ALL.iter().enumerate() {
            assert_eq!(position(*locale), index, "{locale:?}");
        }
    }

    #[test]
    fn locale_aware_set_is_exactly_the_fake_backed_types() {
        let aware = [
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
            DataType::Username,
            DataType::Company,
            DataType::JobTitle,
            DataType::Industry,
        ];
        for data_type in &aware {
            assert!(data_type.is_locale_aware(), "{data_type:?}");
        }

        let unaware = [
            DataType::Birthday,
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
            DataType::Product,
            DataType::ProductDescription,
            DataType::Buzzword,
        ];
        for data_type in &unaware {
            assert!(!data_type.is_locale_aware(), "{data_type:?}");
        }
    }
}
