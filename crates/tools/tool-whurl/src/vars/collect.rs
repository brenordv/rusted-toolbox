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
