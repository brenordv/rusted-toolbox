#[derive(Debug)]
pub struct DistroCcRuntimeConfig {
    pub from: String,
    pub to: Option<String>,
    pub command: String,
    pub no_header: bool,
    pub verbose: bool,
}

impl DistroCcRuntimeConfig {
    pub fn new(
        from: String,
        to: Option<String>,
        command: String,
        no_header: bool,
        verbose: bool,
    ) -> Self {
        Self {
            from,
            to,
            command,
            no_header,
            verbose,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroFamily {
    Debian,
    Arch,
}

impl DistroFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            DistroFamily::Debian => "debian",
            DistroFamily::Arch => "arch",
        }
    }
}
