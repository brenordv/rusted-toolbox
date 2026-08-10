use std::net::IpAddr;

#[derive(Clone, Debug, PartialEq)]
pub enum IpMode {
    Auto,
    V4,
    V6,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutputMode {
    Default,
    Json,
    Csv,
    Template(String),
}

#[derive(Clone, Debug)]
pub struct PingxArgs {
    pub target: String,
    pub count: i64,
    pub interval_secs: f64,
    pub payload_size_bytes: usize,
    pub per_reply_timeout_secs: f64,
    pub overall_deadline_secs: Option<f64>,
    pub continuous: bool,
    pub ip_mode: IpMode,
    pub timestamp_prefix: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub numeric: bool,
    pub output: OutputMode,
    pub stats_every_secs: Option<f64>,
    pub beep_on_loss: bool,
    pub compact_header: bool,
    pub no_header: bool,
    pub stop_on_error: bool, // stop on the first error when running with only default options
}

impl PingxArgs {
    pub fn is_infinite(&self) -> bool {
        self.count < 0 || self.continuous
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedTargetInfo {
    pub host: String,
    pub ip: IpAddr,
    pub reverse_dns: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PacketResult {
    pub icmp_seq: u64,
    pub time_ms: f64,
    pub error: Option<String>,
}
