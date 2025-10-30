use crate::models::{IpMode, OutputMode, PingxArgs, ResolvedTargetInfo};
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::net::IpAddr;

pub fn get_cli_arguments() -> anyhow::Result<PingxArgs> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Cross platform CLI tool to ping other hosts. Like ping, but with extra functionalities, for convenience.",
        )
        .arg(
            Arg::new("target")
                .help("Hostname or IP address to ping")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::new("count")
                .short('c')
                .long("count")
                .value_name("N")
                .help("Number of packets to send (-1 for infinite)")
                .value_parser(clap::value_parser!(i64))
                .required(false),
        )
        .arg(
            Arg::new("interval")
                .short('i')
                .long("interval")
                .value_name("SECS")
                .help("Interval between packets in seconds (e.g., 0.5)")
                .value_parser(clap::value_parser!(f64))
                .required(false),
        )
        .arg(
            Arg::new("size")
                .short('s')
                .long("size")
                .value_name("BYTES")
                .help("ICMP payload size in bytes (default 56)")
                .value_parser(clap::value_parser!(usize))
                .required(false),
        )
        .arg(
            Arg::new("ttl")
                .short('t')
                .long("ttl")
                .value_name("HOPS")
                .help("Set Time To Live hop limit")
                .value_parser(clap::value_parser!(u8))
                .required(false),
        )
        .arg(
            Arg::new("timeout")
                .short('w')
                .long("timeout")
                .value_name("SECS")
                .help("Per reply timeout in seconds")
                .value_parser(clap::value_parser!(f64))
                .required(false),
        )
        .arg(
            Arg::new("deadline")
                .short('W')
                .long("deadline")
                .value_name("SECS")
                .help("Stop after total elapsed seconds")
                .value_parser(clap::value_parser!(f64))
                .required(false),
        )
        .arg(
            Arg::new("continuous")
                .short('T')
                .long("continuous")
                .action(clap::ArgAction::SetTrue)
                .help("Ping until interrupted"),
        )
        .arg(Arg::new("ipv4").short('4').long("ipv4").action(clap::ArgAction::SetTrue).help("Force IPv4"))
        .arg(Arg::new("ipv6").short('6').long("ipv6").action(clap::ArgAction::SetTrue).help("Force IPv6"))
        .arg(
            Arg::new("source")
                .short('S')
                .long("source")
                .value_name("IP")
                .help("Specify source IP to bind from")
                .required(false),
        )
        .arg(Arg::new("timestamp").short('D').long("timestamp").action(clap::ArgAction::SetTrue).help("Prefix each reply with timestamp"))
        .arg(Arg::new("quiet").short('q').long("quiet").action(clap::ArgAction::SetTrue).help("Quiet mode: only summary"))
        .arg(Arg::new("verbose").short('v').long("verbose").action(clap::ArgAction::SetTrue).help("Verbose output"))
        .arg(Arg::new("numeric").short('n').long("numeric").action(clap::ArgAction::SetTrue).help("Don't resolve reverse DNS"))
        .arg(Arg::new("no-fragment").short('f').long("no-fragment").action(clap::ArgAction::SetTrue).help("Don't Fragment (IPv4)"))
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("MODE|TEMPLATE")
                .help("Output: default|json|csv or custom template")
                .required(false),
        )
        .arg(
            Arg::new("stats-every")
                .short('e')
                .long("stats-every")
                .value_name("SECS")
                .help("Print stats every N seconds")
                .value_parser(clap::value_parser!(f64))
                .required(false),
        )
        .arg(Arg::new("beep").short('b').long("beep").action(clap::ArgAction::SetTrue).help("Beep on packet loss"))
        .arg(Arg::new("compact-header").short('m').long("compact-header").action(clap::ArgAction::SetTrue).help("Print compact header"))
        .arg(Arg::new("no-header").short('p').long("no-header").action(clap::ArgAction::SetTrue).help("Do not print header"))
        .get_matches();

    let target = matches.get_one::<String>("target").unwrap().to_string();
    let count = matches.get_one::<i64>("count").copied().unwrap_or(-1);
    if count == 0 || count < -1 {
        anyhow::bail!("--count must be -1 (for infinite, but in this case you can also use --continuous) or >= 1");
    }
    let interval_secs = matches.get_one::<f64>("interval").copied().unwrap_or(1.0);
    let payload_size_bytes = matches.get_one::<usize>("size").copied().unwrap_or(56);
    let ttl = matches.get_one::<u8>("ttl").copied();
    let per_reply_timeout_secs = matches.get_one::<f64>("timeout").copied().unwrap_or(2.0);
    let overall_deadline_secs = matches.get_one::<f64>("deadline").copied();
    let continuous = matches.get_flag("continuous");
    let ip_mode = match (matches.get_flag("ipv4"), matches.get_flag("ipv6")) {
        (true, true) => anyhow::bail!("--ipv4 and --ipv6 are mutually exclusive"),
        (true, false) => IpMode::V4,
        (false, true) => IpMode::V6,
        (false, false) => IpMode::Auto,
    };
    let source = matches
        .get_one::<String>("source")
        .map(|s| s.parse::<IpAddr>())
        .transpose()
        .map_err(|_| anyhow::anyhow!("Invalid --source IP address"))?;
    let timestamp_prefix = matches.get_flag("timestamp");
    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_flag("verbose");
    let numeric = matches.get_flag("numeric");
    let dont_fragment = matches.get_flag("no-fragment");
    let output = match matches
        .get_one::<String>("output")
        .map(|s| s.to_lowercase())
    {
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
            OutputMode::Template(template.clone())
        }
    };
    let stats_every_secs = matches.get_one::<f64>("stats-every").copied();
    let beep_on_loss = matches.get_flag("beep");
    let compact_header = matches.get_flag("compact-header");
    let no_header = matches.get_flag("no-header");

    let explicit_count_inf = matches.value_source("count").is_some() && count == -1;

    let stop_on_error = continuous || explicit_count_inf;

    Ok(PingxArgs {
        target,
        count,
        interval_secs,
        payload_size_bytes,
        ttl,
        per_reply_timeout_secs,
        overall_deadline_secs,
        continuous,
        ip_mode,
        source,
        timestamp_prefix,
        quiet,
        verbose,
        numeric,
        dont_fragment,
        output,
        stats_every_secs,
        beep_on_loss,
        compact_header,
        no_header,
        stop_on_error,
    })
}

pub fn print_header(args: &PingxArgs, resolved: &ResolvedTargetInfo) {
    if args.no_header || args.quiet {
        return;
    }

    if args.compact_header {
        let header_size = if resolved.ip.is_ipv4() {
            20 + 8
        } else {
            40 + 8
        };
        println!(
            "PING {} ({}) {}({}) bytes of data.",
            resolved.host,
            resolved.ip,
            args.payload_size_bytes,
            args.payload_size_bytes + header_size
        );
        return;
    }

    println!("XPing v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("- Host: {}", resolved.host);
    println!("- IP: {}", resolved.ip);
    println!(
        "- Reverse DNS: {}",
        resolved.reverse_dns.as_deref().unwrap_or("(disabled)")
    );
    let header_size = if resolved.ip.is_ipv4() {
        20 + 8
    } else {
        40 + 8
    };
    println!(
        "- Packet Size: {} (with headers: {})",
        args.payload_size_bytes,
        args.payload_size_bytes + header_size
    );
    if args.is_infinite() {
        println!("- Continuous mode");
    } else {
        println!("- Count: {}", args.count);
    }
    println!();
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
        "%ttl%",
        "%time%",
        "%timestamp%",
        "%error%",
    ];
    tags.iter().any(|tag| t.contains(tag))
}
