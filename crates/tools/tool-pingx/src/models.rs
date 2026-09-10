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
    /// The wrapped 16-bit value that also goes on the wire.
    pub icmp_seq: u16,
    pub time_ms: f64,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(count: i64, continuous: bool) -> PingxArgs {
        PingxArgs {
            target: "example.com".to_string(),
            count,
            interval_secs: 1.0,
            payload_size_bytes: 56,
            per_reply_timeout_secs: 2.0,
            overall_deadline_secs: None,
            continuous,
            ip_mode: IpMode::Auto,
            timestamp_prefix: false,
            quiet: false,
            verbose: false,
            numeric: false,
            output: OutputMode::Default,
            stats_every_secs: None,
            beep_on_loss: false,
            stop_on_error: true,
        }
    }

    #[test]
    fn negative_count_is_infinite() {
        assert!(args(-1, false).is_infinite());
    }

    #[test]
    fn continuous_flag_is_infinite() {
        assert!(args(3, true).is_infinite());
    }

    #[test]
    fn positive_count_without_continuous_is_finite() {
        assert!(!args(3, false).is_infinite());
    }
}
