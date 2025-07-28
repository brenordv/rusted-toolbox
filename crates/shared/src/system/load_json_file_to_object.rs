use anyhow::{anyhow, Result};
use serde::de::DeserializeOwned;
use std::path::Path;
use tokio::fs;

/// Loads a JSON file and deserializes it into the specified type.
///
/// # Arguments
/// * `path` - The path to the JSON file to load
///
/// # Returns
/// * `Result<T>` - The deserialized object of type T on success, or an error
///
/// # Errors
/// * Returns an error if the file cannot be read
/// * Returns an error if the JSON cannot be parsed
/// * Returns an error if the JSON structure doesn't match the expected type T
pub async fn load_json_file_to_object<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = fs::read_to_string(path)
        .await
        .map_err(|e| anyhow!("Failed to read file {:?}: {}", path, e))?;

    // Remove a leading UTF-8 BOM if present
    let content = content.trim_start_matches('\u{FEFF}');

    let object: T = serde_json::from_str(content)
        .map_err(|e| anyhow!("Failed to parse JSON from file {:?}: {}", path, e))?;

    Ok(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use tempfile::{NamedTempFile, TempDir};
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct TestConfig {
        name: String,
        version: String,
        enabled: bool,
        count: i32,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct NestedConfig {
        app: TestConfig,
        metadata: HashMap<String, String>,
        tags: Vec<String>,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct EmptyStruct {}

    async fn create_temp_json_file(content: &str) -> Result<NamedTempFile> {
        let temp_file = NamedTempFile::new()?;
        let mut file = File::create(temp_file.path()).await?;
        file.write_all(content.as_bytes()).await?;
        file.flush().await?;
        Ok(temp_file)
    }

    #[tokio::test]
    async fn test_load_simple_json_object() {
        let json_content = r#"
        {
            "name": "test-app",
            "version": "1.0.0",
            "enabled": true,
            "count": 42
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: TestConfig = load_json_file_to_object(temp_file.path()).await.unwrap();

        let expected = TestConfig {
            name: "test-app".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            count: 42,
        };

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_load_nested_json_object() {
        let json_content = r#"
        {
            "app": {
                "name": "nested-app",
                "version": "2.0.0",
                "enabled": false,
                "count": -10
            },
            "metadata": {
                "author": "test-author",
                "environment": "test"
            },
            "tags": ["rust", "json", "test"]
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: NestedConfig = load_json_file_to_object(temp_file.path()).await.unwrap();

        let expected = NestedConfig {
            app: TestConfig {
                name: "nested-app".to_string(),
                version: "2.0.0".to_string(),
                enabled: false,
                count: -10,
            },
            metadata: {
                let mut map = HashMap::new();
                map.insert("author".to_string(), "test-author".to_string());
                map.insert("environment".to_string(), "test".to_string());
                map
            },
            tags: vec!["rust".to_string(), "json".to_string(), "test".to_string()],
        };

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_load_empty_json_object() {
        let json_content = "{}";

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: EmptyStruct = load_json_file_to_object(temp_file.path()).await.unwrap();

        assert_eq!(result, EmptyStruct {});
    }

    #[tokio::test]
    async fn test_load_json_with_unicode_characters() {
        let json_content = r#"
        {
            "name": "测试应用",
            "version": "1.0.0-αβγ",
            "enabled": true,
            "count": 123
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: TestConfig = load_json_file_to_object(temp_file.path()).await.unwrap();

        let expected = TestConfig {
            name: "测试应用".to_string(),
            version: "1.0.0-αβγ".to_string(),
            enabled: true,
            count: 123,
        };

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_load_json_with_special_characters() {
        let json_content = r#"
        {
            "name": "app-with-\"quotes\"-and-\\backslashes",
            "version": "1.0.0\n\t\r",
            "enabled": true,
            "count": 0
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: TestConfig = load_json_file_to_object(temp_file.path()).await.unwrap();

        let expected = TestConfig {
            name: "app-with-\"quotes\"-and-\\backslashes".to_string(),
            version: "1.0.0\n\t\r".to_string(),
            enabled: true,
            count: 0,
        };

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_load_json_array_as_vec() {
        let json_content = r#"["first", "second", "third"]"#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: Vec<String> = load_json_file_to_object(temp_file.path()).await.unwrap();

        let expected = vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ];
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_load_json_primitive_types() {
        // Test string
        let temp_file = create_temp_json_file(r#""hello world""#).await.unwrap();
        let result: String = load_json_file_to_object(temp_file.path()).await.unwrap();
        assert_eq!(result, "hello world");

        // Test number
        let temp_file = create_temp_json_file("42").await.unwrap();
        let result: i32 = load_json_file_to_object(temp_file.path()).await.unwrap();
        assert_eq!(result, 42);

        // Test boolean
        let temp_file = create_temp_json_file("true").await.unwrap();
        let result: bool = load_json_file_to_object(temp_file.path()).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_load_json_with_null_values() {
        #[derive(Debug, Deserialize, Serialize, PartialEq)]
        struct ConfigWithOptional {
            name: String,
            optional_field: Option<String>,
            another_optional: Option<i32>,
        }

        let json_content = r#"
        {
            "name": "test",
            "optional_field": null,
            "another_optional": 42
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: ConfigWithOptional = load_json_file_to_object(temp_file.path()).await.unwrap();

        let expected = ConfigWithOptional {
            name: "test".to_string(),
            optional_field: None,
            another_optional: Some(42),
        };

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_file_not_found_error() {
        let non_existent_path = Path::new("/non/existent/path.json");
        let result: Result<TestConfig> = load_json_file_to_object(non_existent_path).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to read file"));
        assert!(error.to_string().contains("/non/existent/path.json"));
    }

    #[tokio::test]
    async fn test_invalid_json_syntax_error() {
        let invalid_json = r#"{ "name": "test", "invalid": }"#; // Missing value after colon

        let temp_file = create_temp_json_file(invalid_json).await.unwrap();
        let result: Result<TestConfig> = load_json_file_to_object(temp_file.path()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_json_type_mismatch_error() {
        let json_content = r#"
        {
            "name": "test",
            "version": "1.0.0",
            "enabled": "not_a_boolean",
            "count": 42
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: Result<TestConfig> = load_json_file_to_object(temp_file.path()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_missing_required_field_error() {
        let json_content = r#"
        {
            "name": "test",
            "version": "1.0.0"
        }
        "#; // Missing required fields "enabled" and "count"

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: Result<TestConfig> = load_json_file_to_object(temp_file.path()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_empty_file_error() {
        let temp_file = create_temp_json_file("").await.unwrap();
        let result: Result<TestConfig> = load_json_file_to_object(temp_file.path()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_whitespace_only_file_error() {
        let temp_file = create_temp_json_file("   \n\t\r\n   ").await.unwrap();
        let result: Result<TestConfig> = load_json_file_to_object(temp_file.path()).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_large_json_file() {
        // Create a large JSON object
        let mut large_json = String::from("{\n");
        for i in 0..1000 {
            large_json.push_str(&format!("  \"field_{}\": \"value_{}\",\n", i, i));
        }
        large_json.push_str("  \"name\": \"large-test\",\n");
        large_json.push_str("  \"version\": \"1.0.0\",\n");
        large_json.push_str("  \"enabled\": true,\n");
        large_json.push_str("  \"count\": 9999\n");
        large_json.push('}');

        let temp_file = create_temp_json_file(&large_json).await.unwrap();

        // Use HashMap for dynamic field handling
        let result: HashMap<String, serde_json::Value> =
            load_json_file_to_object(temp_file.path()).await.unwrap();

        assert_eq!(result.get("name").unwrap(), "large-test");
        assert_eq!(result.get("version").unwrap(), "1.0.0");
        assert_eq!(result.get("enabled").unwrap(), &true);
        assert_eq!(result.get("count").unwrap(), &9999);
        assert!(result.len() > 1000); // Should have many fields
    }

    #[tokio::test]
    async fn test_json_with_deeply_nested_structure() {
        let json_content = r#"
        {
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "level5": {
                                "deep_value": "found_it"
                            }
                        }
                    }
                }
            }
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: serde_json::Value = load_json_file_to_object(temp_file.path()).await.unwrap();

        let deep_value =
            result["level1"]["level2"]["level3"]["level4"]["level5"]["deep_value"].as_str();
        assert_eq!(deep_value, Some("found_it"));
    }

    #[tokio::test]
    async fn test_json_with_various_number_formats() {
        #[derive(Debug, Deserialize, Serialize, PartialEq)]
        struct NumberConfig {
            integer: i64,
            float: f64,
            negative: i32,
            zero: i32,
            scientific: f64,
        }

        let json_content = r#"
        {
            "integer": 9223372036854775807,
            "float": 3.141592653589793,
            "negative": -42,
            "zero": 0,
            "scientific": 1.23e-4
        }
        "#;

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: NumberConfig = load_json_file_to_object(temp_file.path()).await.unwrap();

        assert_eq!(result.integer, 9223372036854775807);
        assert!((result.float - std::f64::consts::PI).abs() < 1e-10);
        assert_eq!(result.negative, -42);
        assert_eq!(result.zero, 0);
        assert!((result.scientific - 0.000123).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_concurrent_file_loading() {
        use futures::future::join_all;

        let json_content = r#"
        {
            "name": "concurrent-test",
            "version": "1.0.0",
            "enabled": true,
            "count": 100
        }
        "#;

        // Create multiple temp files
        let mut tasks = Vec::new();
        for i in 0..10 {
            let content =
                json_content.replace("concurrent-test", &format!("concurrent-test-{}", i));
            let content = content.replace("100", &format!("{}", i * 10));

            tasks.push(async move {
                let temp_file = create_temp_json_file(&content).await.unwrap();
                let result: TestConfig = load_json_file_to_object(temp_file.path()).await.unwrap();
                (i, result)
            });
        }

        let results = join_all(tasks).await;

        // Verify all results
        assert_eq!(results.len(), 10);
        for (i, result) in results {
            assert_eq!(result.name, format!("concurrent-test-{}", i));
            assert_eq!(result.count, i * 10);
        }
    }

    #[tokio::test]
    async fn test_file_in_subdirectory() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        tokio::fs::create_dir(&subdir).await.unwrap();

        let file_path = subdir.join("config.json");
        let json_content = r#"
        {
            "name": "subdir-test",
            "version": "1.0.0",
            "enabled": true,
            "count": 42
        }
        "#;

        tokio::fs::write(&file_path, json_content).await.unwrap();

        let result: TestConfig = load_json_file_to_object(&file_path).await.unwrap();

        assert_eq!(result.name, "subdir-test");
        assert_eq!(result.version, "1.0.0");
        assert!(result.enabled);
        assert_eq!(result.count, 42);
    }

    #[tokio::test]
    async fn test_file_with_bom() {
        // Create file with UTF-8 BOM
        let json_content = "\u{FEFF}{\n  \"name\": \"bom-test\",\n  \"version\": \"1.0.0\",\n  \"enabled\": true,\n  \"count\": 42\n}";

        let temp_file = create_temp_json_file(json_content).await.unwrap();
        let result: TestConfig = load_json_file_to_object(temp_file.path()).await.unwrap();

        assert_eq!(result.name, "bom-test");
        assert_eq!(result.version, "1.0.0");
        assert!(result.enabled);
        assert_eq!(result.count, 42);
    }
}
