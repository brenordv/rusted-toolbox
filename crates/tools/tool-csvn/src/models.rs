use std::collections::HashMap;
use std::path::PathBuf;

/// CSV normalization configuration.
///
/// Contains the input file path, headers, default mappings, and processing options.
#[derive(Debug, Clone)]
pub struct CsvNConfig {
    pub input_file: PathBuf,
    pub headers: Option<Vec<String>>,
    pub clean_string: bool,
    pub default_value_map: HashMap<String, String>,
    pub feedback_interval: usize,
}

impl CsvNConfig {
    pub fn new(
        input_file: PathBuf,
        headers: Option<Vec<String>>,
        clean_string: bool,
        default_value_map: HashMap<String, String>,
        feedback_interval: usize,
    ) -> Self {
        Self {
            input_file,
            headers,
            clean_string,
            default_value_map,
            feedback_interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(config.default_value_map.is_empty());
    }

    #[test]
    fn new_stores_default_value_map() {
        let map = HashMap::from([("name".to_string(), "unknown".to_string())]);

        let config = CsvNConfig::new(PathBuf::from("data.csv"), None, false, map, 100);

        assert_eq!(
            config.default_value_map.get("name").map(String::as_str),
            Some("unknown")
        );
    }
}
