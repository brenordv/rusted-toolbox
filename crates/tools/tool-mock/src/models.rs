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
