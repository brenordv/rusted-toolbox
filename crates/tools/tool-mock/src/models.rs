use mock_data_utils::models::{DataType, Locale, MockOptions};

/// Command-line arguments for mock data generator
#[derive(Debug)]
pub struct MockConfig {
    pub data_type: DataType,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub length: Option<usize>,
    pub precision: Option<u32>,
    pub age: Option<u32>,
    pub past: bool,
    pub future: bool,
    pub range: Option<u32>,
    pub locale: Locale,
}

pub trait FromArgs {
    fn from_args(args: &MockConfig) -> Self;
}

impl FromArgs for MockOptions {
    /// Create MockOptions from command-line arguments
    fn from_args(args: &MockConfig) -> Self {
        MockOptions {
            data_type: args.data_type.clone(),
            min: args.min,
            max: args.max,
            length: args.length,
            precision: args.precision,
            age: args.age,
            past: args.past,
            future: args.future,
            range: args.range,
            locale: args.locale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_args_maps_every_populated_field() {
        let config = MockConfig {
            data_type: DataType::Float,
            min: Some(-3),
            max: Some(42),
            length: Some(8),
            precision: Some(2),
            age: Some(30),
            past: true,
            future: false,
            range: Some(5),
            locale: Locale::PtBr,
        };

        let options = MockOptions::from_args(&config);

        assert!(matches!(options.data_type, DataType::Float));
        assert_eq!(options.min, Some(-3));
        assert_eq!(options.max, Some(42));
        assert_eq!(options.length, Some(8));
        assert_eq!(options.precision, Some(2));
        assert_eq!(options.age, Some(30));
        assert!(options.past);
        assert!(!options.future);
        assert_eq!(options.range, Some(5));
        assert_eq!(options.locale, Locale::PtBr);
    }

    #[test]
    fn from_args_maps_empty_fields_and_future_flag() {
        let config = MockConfig {
            data_type: DataType::Timestamp,
            min: None,
            max: None,
            length: None,
            precision: None,
            age: None,
            past: false,
            future: true,
            range: None,
            locale: Locale::En,
        };

        let options = MockOptions::from_args(&config);

        assert!(matches!(options.data_type, DataType::Timestamp));
        assert_eq!(options.min, None);
        assert_eq!(options.max, None);
        assert_eq!(options.length, None);
        assert_eq!(options.precision, None);
        assert_eq!(options.age, None);
        assert!(!options.past);
        assert!(options.future);
        assert_eq!(options.range, None);
        assert_eq!(options.locale, Locale::En);
    }
}
