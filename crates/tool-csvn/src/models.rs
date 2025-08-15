use std::collections::HashMap;
use std::path::PathBuf;
use string_interner::{DefaultBackend, DefaultSymbol, StringInterner};

/// CSV normalization configuration.
///
/// Contains input file path, headers, default mappings, and processing options.
/// Uses string interning for memory optimization.
pub struct CsvNConfig {
    pub input_file: PathBuf,
    pub headers: Option<Vec<String>>,
    pub clean_string: bool,
    pub default_value_map: HashMap<String, String>,
    pub feedback_interval: u64,
    pub string_interner: StringInterner<DefaultBackend>,
    pub interned_defaults: HashMap<String, DefaultSymbol>,
}

impl CsvNConfig {
    pub fn new(
        input_file: PathBuf,
        headers: Option<Vec<String>>,
        clean_string: bool,
        default_value_map: HashMap<String, String>,
        feedback_interval: u64,
    ) -> Self {
        let mut interner = StringInterner::<DefaultBackend>::new();
        let mut interned_defaults = HashMap::new();

        // Pre-intern all default values
        for (key, value) in &default_value_map {
            let symbol = interner.get_or_intern(value);
            interned_defaults.insert(key.clone(), symbol);
        }

        Self {
            input_file,
            headers,
            clean_string,
            default_value_map,
            feedback_interval,
            string_interner: interner,
            interned_defaults,
        }
    }
}