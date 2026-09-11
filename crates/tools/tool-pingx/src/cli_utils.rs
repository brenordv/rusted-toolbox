use crate::models::{IpMode, OutputMode, PingxArgs, ResolvedTargetInfo};
use anyhow::Result;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::{
    format_config_item, format_config_item_level3, format_config_label,
};

/// Cross-platform CLI tool to ping other hosts.
///
/// Like ping, but with extra functionalities, for convenience.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Hostname or IP address to ping
    pub target: String,

    /// Number of packets to send (-1 for infinite)
    #[arg(short = 'c', long = "count", value_name = "N")]
    pub count: Option<i64>,

    /// Interval between packets in seconds (e.g., 0.5)
    #[arg(short = 'i', long = "interval", value_name = "SECS")]
    pub interval: Option<f64>,

    /// ICMP payload size in bytes (default 56)
    #[arg(short = 's', long = "size", value_name = "BYTES")]
    pub size: Option<usize>,

    /// Per reply timeout in seconds
    #[arg(short = 'w', long = "timeout", value_name = "SECS")]
    pub timeout: Option<f64>,

    /// Stop after total elapsed seconds
    #[arg(short = 'W', long = "deadline", value_name = "SECS")]
    pub deadline: Option<f64>,

    /// Ping until interrupted, even if the host is unreachable
    #[arg(short = 'T', long = "continuous")]
    pub continuous: bool,

    /// Force IPv4
    #[arg(short = '4', long = "ipv4", conflicts_with = "ipv6")]
    pub ipv4: bool,

    /// Force IPv6
    #[arg(short = '6', long = "ipv6", conflicts_with = "ipv4")]
    pub ipv6: bool,

    /// Prefix each reply with timestamp
    #[arg(short = 'D', long = "timestamp")]
    pub timestamp: bool,

    /// Quiet mode: only summary
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Don't resolve reverse DNS
    #[arg(short = 'n', long = "numeric")]
    pub numeric: bool,

    /// Output: default|json|csv or custom template
    #[arg(short = 'o', long = "output", value_name = "MODE|TEMPLATE")]
    pub output: Option<String>,

    /// Print stats every N seconds
    #[arg(short = 'e', long = "stats-every", value_name = "SECS")]
    pub stats_every: Option<f64>,

    /// Beep on packet loss
    #[arg(short = 'b', long = "beep")]
    pub beep: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
///
/// # Errors
/// Returns an error when `--ipv4` and `--ipv6` are combined, when `--count` is
/// outside the accepted range, or when an `--output` template contains no tags.
pub fn initialize() -> Result<PingxArgs> {
    let args = CliArgs::parse();

    let config = build_config(&args)?;

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        true,
        false,
        Some(|| {
            print_header(&config);
        }),
    );

    Ok(config)
}

/// Validates the parsed arguments and resolves them into a [`PingxArgs`].
fn build_config(args: &CliArgs) -> Result<PingxArgs> {
    let ip_mode = match (args.ipv4, args.ipv6) {
        (true, true) => anyhow::bail!("--ipv4 and --ipv6 are mutually exclusive"),
        (true, false) => IpMode::V4,
        (false, true) => IpMode::V6,
        (false, false) => IpMode::Auto,
    };

    let output = match args.output.as_deref().map(|s| s.to_lowercase()) {
        None => OutputMode::Default,
        Some(ref s) if s == "default" => OutputMode::Default,
        Some(ref s) if s == "json" => OutputMode::Json,
        Some(ref s) if s == "csv" => OutputMode::Csv,
        Some(template) => {
            if !template_has_any_tag(&template) {
                anyhow::bail!(
                    "Invalid --output template: must contain at least one tag like %host%, %ip%, %time%"
                );
            }
            OutputMode::Template(template)
        }
    };

    let count = args.count.unwrap_or(-1);
    if count == 0 || count < -1 {
        anyhow::bail!("--count must be -1 (for infinite, but in this case you can also use --continuous) or >= 1");
    }

    let explicit_count_inf = args.count.is_some() && count == -1;
    let continuous = args.continuous;
    let stop_on_error = !continuous && !explicit_count_inf;

    Ok(PingxArgs {
        target: args.target.clone(),
        count,
        interval_secs: args.interval.unwrap_or(1.0),
        payload_size_bytes: args.size.unwrap_or(56),
        per_reply_timeout_secs: args.timeout.unwrap_or(2.0),
        overall_deadline_secs: args.deadline,
        continuous,
        ip_mode,
        timestamp_prefix: args.timestamp,
        quiet: args.quiet,
        verbose: args.common.verbose,
        numeric: args.numeric,
        output,
        stats_every_secs: args.stats_every,
        beep_on_loss: args.beep,
        stop_on_error,
    })
}

pub fn print_supplemental_header(args: &PingxArgs, resolved_target_info: &ResolvedTargetInfo) {
    println!("{}", format_config_label("Resolved Target"));
    println!(
        "{}",
        format_config_item_level3("Host", &resolved_target_info.host)
    );
    println!(
        "{}",
        format_config_item_level3("IP", resolved_target_info.ip)
    );
    println!(
        "{}",
        format_config_item_level3(
            "Reverse DNS",
            resolved_target_info
                .reverse_dns
                .as_deref()
                .unwrap_or("(disabled)")
        )
    );

    let header_size = if resolved_target_info.ip.is_ipv4() {
        20 + 8
    } else {
        40 + 8
    };

    println!(
        "{}",
        format_config_item_level3(
            "Payload size",
            format!(
                "{} (with IP+ICMP headers: {})",
                args.payload_size_bytes,
                args.payload_size_bytes + header_size
            )
        )
    );
}

fn print_header(args: &PingxArgs) {
    println!("{}", format_config_item("Host", &args.target));

    if args.is_infinite() {
        println!("{}", format_config_label("Continuous mode"));
    } else {
        println!("{}", format_config_item("Count", args.count));
    }

    println!(
        "{}",
        format_config_item("Interval", format!("{} seconds", args.interval_secs))
    );

    println!(
        "{}",
        format_config_item(
            "Timeout",
            format!("{} seconds", args.per_reply_timeout_secs)
        )
    );

    if let Some(deadline) = args.overall_deadline_secs {
        println!(
            "{}",
            format_config_item("Stop after total elapsed", format!("{} seconds", deadline))
        );
    }

    println!(
        "{}",
        format_config_item("Stop on error", args.stop_on_error)
    );

    let output_mode = match &args.output {
        OutputMode::Default => "default".to_string(),
        OutputMode::Json => "json".to_string(),
        OutputMode::Csv => "csv".to_string(),
        OutputMode::Template(template) => format!("template: {}", template),
    };

    println!("{}", format_config_item("Output", output_mode));
}

fn template_has_any_tag(template: &str) -> bool {
    let t = template.to_ascii_lowercase();
    let tags = [
        "%host%",
        "%ip%",
        "%reverse_dns%",
        "%size%",
        "%size_no_headers%",
        "%icmp_seq%",
        "%time%",
        "%timestamp%",
        "%error%",
    ];
    tags.iter().any(|tag| t.contains(tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn ipv4_and_ipv6_are_mutually_exclusive() {
        // clap rejects the combination at parse time via `conflicts_with`;
        // build_config keeps its own guard for the same pair.
        assert!(CliArgs::try_parse_from(["pingx", "host", "-4", "-6"]).is_err());
    }

    #[test]
    fn count_zero_is_rejected() {
        let args = CliArgs::try_parse_from(["pingx", "host", "-c", "0"]).unwrap();
        assert!(build_config(&args).is_err());
    }

    #[test]
    fn defaults_are_applied() {
        let args = CliArgs::try_parse_from(["pingx", "host"]).unwrap();
        let config = build_config(&args).unwrap();
        assert_eq!(config.count, -1);
        assert_eq!(config.interval_secs, 1.0);
        assert_eq!(config.payload_size_bytes, 56);
        assert_eq!(config.per_reply_timeout_secs, 2.0);
        assert_eq!(config.ip_mode, IpMode::Auto);
        assert_eq!(config.output, OutputMode::Default);
        assert!(config.stop_on_error);
    }

    #[test]
    fn explicit_infinite_count_disables_stop_on_error() {
        let args = CliArgs::try_parse_from(["pingx", "host", "-c=-1"]).unwrap();
        let config = build_config(&args).unwrap();
        assert!(!config.stop_on_error);
    }

    #[test]
    fn output_template_requires_a_tag() {
        let bad = CliArgs::try_parse_from(["pingx", "host", "-o", "no tags here"]).unwrap();
        assert!(build_config(&bad).is_err());

        let good = CliArgs::try_parse_from(["pingx", "host", "-o", "%host% %time%"]).unwrap();
        let config = build_config(&good).unwrap();
        assert!(matches!(config.output, OutputMode::Template(_)));
    }

    #[test]
    fn template_has_any_tag_detects_known_tags() {
        assert!(template_has_any_tag("ping %host% now"));
        assert!(template_has_any_tag("%error%"));
    }

    #[test]
    fn template_has_any_tag_rejects_untagged_text() {
        assert!(!template_has_any_tag("no tags here"));
        assert!(!template_has_any_tag("%unknown%"));
    }

    #[test]
    fn template_has_any_tag_is_case_insensitive() {
        assert!(template_has_any_tag("%HOST% and %Time%"));
    }
}
