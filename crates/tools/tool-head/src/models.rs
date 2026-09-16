use shared_head_tail::models::{delimiter_for, CountUnit, HeaderPolicy};

/// Runtime configuration for the head engine.
#[derive(Debug, Clone)]
pub struct HeadConfig {
    pub unit: CountUnit,
    pub count: u64,
    /// `true` for `-n -NUM`/`-c -NUM`: print all but the last NUM items.
    pub elide: bool,
    pub headers: HeaderPolicy,
    pub zero_terminated: bool,
    pub files: Vec<String>,
}

impl HeadConfig {
    pub fn delimiter(&self) -> u8 {
        delimiter_for(self.zero_terminated)
    }
}
