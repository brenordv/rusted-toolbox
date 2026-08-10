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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pre_interns_default_values() {
        let mut map = HashMap::new();
        map.insert("name".to_string(), "unknown".to_string());
        map.insert("city".to_string(), "n/a".to_string());

        let config = CsvNConfig::new(PathBuf::from("data.csv"), None, false, map, 100);

        let name_sym = *config.interned_defaults.get("name").unwrap();
        let city_sym = *config.interned_defaults.get("city").unwrap();
        assert_eq!(config.string_interner.resolve(name_sym).unwrap(), "unknown");
        assert_eq!(config.string_interner.resolve(city_sym).unwrap(), "n/a");
    }

    #[test]
    fn new_keeps_scalar_fields() {
        let config = CsvNConfig::new(
            PathBuf::from("in.csv"),
            Some(vec!["a".to_string()]),
            true,
            HashMap::new(),
            250,
        );

        assert_eq!(config.input_file, PathBuf::from("in.csv"));
        assert_eq!(config.headers, Some(vec!["a".to_string()]));
        assert!(config.clean_string);
        assert_eq!(config.feedback_interval, 250);
        assert!(config.interned_defaults.is_empty());
    }
}
