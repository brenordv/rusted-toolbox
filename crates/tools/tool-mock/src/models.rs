use mock_data_utils::models::{DataType, MockOptions};

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
}

impl MockConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        data_type: DataType,
        min: Option<i32>,
        max: Option<i32>,
        length: Option<usize>,
        precision: Option<u32>,
        age: Option<u32>,
        past: bool,
        future: bool,
        range: Option<u32>,
    ) -> Self {
        MockConfig {
            data_type,
            min,
            max,
            length,
            precision,
            age,
            past,
            future,
            range,
        }
    }
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_args_maps_every_populated_field() {
        let config = MockConfig::new(
            DataType::Float,
            Some(-3),
            Some(42),
            Some(8),
            Some(2),
            Some(30),
            true,
            false,
            Some(5),
        );

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
    }

    #[test]
    fn from_args_maps_empty_fields_and_future_flag() {
        let config = MockConfig::new(
            DataType::Timestamp,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
            None,
        );

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
    }
}
