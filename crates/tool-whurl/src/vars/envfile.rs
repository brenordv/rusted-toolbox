use camino::Utf8Path;

use super::{VariableError, VariableMap};

pub fn parse_variables_file(path: &Utf8Path) -> Result<VariableMap, VariableError> {
    let contents = std::fs::read_to_string(path).map_err(|source| VariableError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    parse_variables_from_str(path, &contents)
}

fn parse_variables_from_str(path: &Utf8Path, contents: &str) -> Result<VariableMap, VariableError> {
    let mut variables = VariableMap::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = raw_line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "expected KEY=VALUE syntax".to_string(),
            });
        };

        let key = key.trim();
        if key.is_empty() {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "variable name cannot be empty".to_string(),
            });
        }

        if key.contains(char::is_whitespace) {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "variable name cannot contain whitespace".to_string(),
            });
        }

        let value = value.trim().to_string();
        variables.insert(key.to_string(), value);
    }

    Ok(variables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    #[test]
    fn parses_variables_file() {
        let path = Utf8PathBuf::from("inline");
        let data = r#"
# comment
FOO = bar
BAR=baz
        "#;

        let parsed = parse_variables_from_str(&path, data).expect("parse");
        assert_eq!(parsed.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(parsed.get("BAR"), Some(&"baz".to_string()));
    }
}
