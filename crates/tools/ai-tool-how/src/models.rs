#[derive(Debug)]
pub struct HowRuntimeConfig {
    pub mode: HowMode,
    pub copy_to_clipboard: bool,
    pub os: String,
    pub shell: Option<String>,
}

#[derive(Debug)]
pub enum HowMode {
    FixCommand(String),
    SuggestCommand(String),
}

impl HowRuntimeConfig {
    pub fn new(mode: HowMode, copy_to_clipboard: bool, os: String, shell: Option<String>) -> Self {
        Self {
            mode,
            copy_to_clipboard,
            os,
            shell,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_all_fields() {
        let config = HowRuntimeConfig::new(
            HowMode::FixCommand("ls -la".to_string()),
            true,
            "linux".to_string(),
            Some("bash".to_string()),
        );

        assert!(config.copy_to_clipboard);
        assert_eq!(config.os, "linux");
        assert_eq!(config.shell, Some("bash".to_string()));
        assert!(matches!(config.mode, HowMode::FixCommand(command) if command == "ls -la"));
    }

    #[test]
    fn new_supports_suggest_mode_without_shell() {
        let config = HowRuntimeConfig::new(
            HowMode::SuggestCommand("find files".to_string()),
            false,
            "windows".to_string(),
            None,
        );

        assert!(!config.copy_to_clipboard);
        assert!(config.shell.is_none());
        assert!(matches!(config.mode, HowMode::SuggestCommand(request) if request == "find files"));
    }
}
