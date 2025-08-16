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
