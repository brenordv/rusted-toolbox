use std::env;

use super::VariableMap;

const ENV_PREFIX: &str = "HURL_";

pub fn gather_process_env_variables() -> VariableMap {
    let mut variables = VariableMap::new();

    for (key, value) in env::vars() {
        if let Some(stripped) = key.strip_prefix(ENV_PREFIX) {
            if stripped.is_empty() {
                continue;
            }

            let normalized = stripped.to_lowercase();
            variables.insert(normalized, value);
        }
    }

    variables
}

pub fn merge_variable_sources(
    env_vars: VariableMap,
    file_vars: Option<VariableMap>,
    inline_vars: &[(String, String)],
) -> VariableMap {
    let mut merged = env_vars;

    if let Some(file_vars) = file_vars {
        for (key, value) in file_vars {
            merged.insert(key, value);
        }
    }

    for (key, value) in inline_vars {
        merged.insert(key.clone(), value.clone());
    }

    merged
}
