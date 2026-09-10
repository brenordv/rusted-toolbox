use crate::cli_utils::print_supplemental_header;
use crate::models::{IpMode, OutputMode, PacketResult, PingxArgs, ResolvedTargetInfo};
use anyhow::Result;
use chrono::Timelike;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use dns_lookup::lookup_addr;
use serde::Serialize;
use std::net::IpAddr;
use surge_ping::{Client, ConfigBuilder, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use tokio::time::{sleep, Duration, Instant};
use tracing::debug;

#[derive(Serialize)]
struct JsonOutput<'a> {
    host: &'a str,
    ip: String,
    reverse_dns: Option<&'a str>,
    size: usize,
    sent: u64,
    received: u64,
    loss_percent: f64,
    packets: Vec<PacketLine>,
}

#[derive(Serialize)]
struct PacketLine {
    icmp_seq: u16,
    time: f64,
}

pub async fn resolve_target(args: &PingxArgs) -> Result<ResolvedTargetInfo> {
    let host = args.target.clone();

    let ip: IpAddr = loop {
        let mut addrs: Vec<IpAddr> = Vec::new();
        let mut last_err: Option<String> = None;

        match tokio::net::lookup_host((host.as_str(), 0)).await {
            Ok(iter) => {
                for s in iter {
                    let ip = s.ip();
                    match args.ip_mode {
                        IpMode::Auto => addrs.push(ip),
                        IpMode::V4 => {
                            if ip.is_ipv4() {
                                addrs.push(ip)
                            }
                        }
                        IpMode::V6 => {
                            if ip.is_ipv6() {
                                addrs.push(ip)
                            }
                        }
                    }
                }
            }
            Err(e) => {
                last_err = Some(format!("DNS resolution failed: {}", e));
            }
        }

        // Fallback: direct parse
        if addrs.is_empty() {
            if let Ok(ip) = host.parse::<IpAddr>() {
                let is_ok = match args.ip_mode {
                    IpMode::Auto => true,
                    IpMode::V4 => ip.is_ipv4(),
                    IpMode::V6 => ip.is_ipv6(),
                };
                if is_ok {
                    addrs.push(ip);
                }
            } else if last_err.is_none() {
                last_err = Some("Failed to parse host as IP address".to_string());
            }
        }

        if let Some(first) = addrs.first().copied() {
            break first;
        }

        // No address resolved; decide whether to stop or keep trying
        if args.stop_on_error {
            let msg = last_err.unwrap_or_else(|| "Failed to resolve target".to_string());
            return Err(anyhow::anyhow!(msg));
        } else {
            if !args.quiet {
                if let Some(msg) = &last_err {
                    eprintln!("resolve error for '{}': {}", host, msg);
                } else {
                    eprintln!("resolve error for '{}': no addresses found", host);
                }
            }
            // simple retry delay
            sleep(Duration::from_secs(1)).await;
        }
    };

    // lookup_addr wraps the synchronous getnameinfo call, so it runs on
    // tokio's blocking pool instead of stalling the async runtime.
    let reverse_dns = if args.numeric {
        None
    } else {
        match tokio::task::spawn_blocking(move || lookup_addr(&ip).ok()).await {
            Ok(name) => name.map(|n| sanitize_display(&n)),
            Err(e) => {
                // An ordinary lookup failure stays silent (None), but a
                // panicked lookup task is a defect signal worth a trace.
                debug!(error = %e, "reverse-DNS lookup task failed");
                None
            }
        }
    };

    Ok(ResolvedTargetInfo {
        host,
        ip,
        reverse_dns,
    })
}

/// Escapes control characters (C0, DEL, C1) and Unicode bidirectional-control
/// characters in a resolved reverse-DNS name. PTR records are
/// attacker-controlled and the name flows into terminal output, CSV, and
/// templates; every other character passes through unchanged.
fn sanitize_display(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let is_bidi_control = matches!(
            c,
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
        );
        if c.is_control() || is_bidi_control {
            out.extend(c.escape_debug());
        } else {
            out.push(c);
        }
    }
    out
}

pub async fn run_ping(args: &PingxArgs) -> Result<()> {
    let resolved = resolve_target(args).await?;
    print_supplemental_header(args, &resolved);

    // Verbose info
    if args.verbose && !args.quiet {
        println!("[verbose] target: {}", args.target);
        println!("[verbose] resolved ip: {}", resolved.ip);
        println!(
            "[verbose] reverse dns: {}",
            resolved.reverse_dns.as_deref().unwrap_or("(none)")
        );
        println!(
            "[verbose] ip mode: {:?}, payload: {} bytes",
            args.ip_mode, args.payload_size_bytes
        );
    }

    // CSV header (once)
    if matches!(args.output, OutputMode::Csv) && !args.quiet {
        if args.timestamp_prefix {
            println!("timestamp,host,ip,reverse_dns,size,icmp_seq,time");
        } else {
            println!("host,ip,reverse_dns,size,icmp_seq,time");
        }
    }

    let shutdown = setup_graceful_shutdown(false);

    let mut config_builder = ConfigBuilder::default();

    let icmp = if resolved.ip.is_ipv4() {
        ICMP::V4
    } else {
        ICMP::V6
    };

    config_builder = config_builder.kind(icmp);

    let config = config_builder.build();

    let client = Client::new(&config)?;
    let mut sequence: u64 = 0;
    let identifier = PingIdentifier(rand_identifier());
    let deadline_start = Instant::now();
    let mut sent: u64 = 0;
    let mut received: u64 = 0;
    let mut lines_for_json: Vec<PacketLine> = Vec::new();

    // stats timer
    let mut next_stats_due = args
        .stats_every_secs
        .map(|e| Instant::now() + Duration::from_secs_f64(e));

    loop {
        if let Some(deadline) = args.overall_deadline_secs {
            if deadline_start.elapsed() >= Duration::from_secs_f64(deadline) {
                break;
            }
        }

        if !args.is_infinite() && sent >= args.count as u64 {
            break;
        }

        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }

        sequence += 1;
        sent += 1;
        // The ICMP sequence field is 16 bits; every output reports this wrapped
        // value so displayed and on-wire sequences stay identical past 65535.
        let wire_seq = sequence as u16;

        let timeout = Duration::from_secs_f64(args.per_reply_timeout_secs);

        let mut pinger = client.pinger(resolved.ip, identifier).await;

        pinger.timeout(timeout);

        let payload = vec![0u8; args.payload_size_bytes];
        let mut had_error = false;
        match pinger.ping(PingSequence(wire_seq), &payload).await {
            Ok((IcmpPacket::V4(_packet), dur)) => {
                received += 1;
                let time_ms = dur.as_secs_f64() * 1000.0;
                let packet_res = PacketResult {
                    icmp_seq: wire_seq,
                    time_ms,
                    error: None,
                };
                print_packet_line(args, &resolved, &packet_res);
                if matches!(args.output, OutputMode::Json) {
                    lines_for_json.push(PacketLine {
                        icmp_seq: wire_seq,
                        time: time_ms,
                    });
                }
            }
            Ok((IcmpPacket::V6(_packet), dur)) => {
                received += 1;
                let time_ms = dur.as_secs_f64() * 1000.0;
                let packet_res = PacketResult {
                    icmp_seq: wire_seq,
                    time_ms,
                    error: None,
                };
                print_packet_line(args, &resolved, &packet_res);
                if matches!(args.output, OutputMode::Json) {
                    lines_for_json.push(PacketLine {
                        icmp_seq: wire_seq,
                        time: time_ms,
                    });
                }
            }
            Err(e) => {
                had_error = true;
                let packet_res = PacketResult {
                    icmp_seq: wire_seq,
                    time_ms: 0.0,
                    error: Some(e.to_string()),
                };
                print_packet_line(args, &resolved, &packet_res);
                if args.beep_on_loss {
                    print!("\x07");
                }
            }
        }
        if had_error && args.stop_on_error {
            break;
        }

        if let Some(every) = args.stats_every_secs {
            if let Some(due) = next_stats_due {
                if Instant::now() >= due {
                    print_stats(args, sent, received);
                    next_stats_due = Some(due + Duration::from_secs_f64(every));
                }
            }
        }

        sleep(Duration::from_secs_f64(args.interval_secs)).await;
    }

    // Final stats
    if matches!(args.output, OutputMode::Default | OutputMode::Csv) {
        print_stats(args, sent, received);
    }

    if matches!(args.output, OutputMode::Json) {
        let loss = loss_percent(sent, received);
        let json = JsonOutput {
            host: &resolved.host,
            ip: resolved.ip.to_string(),
            reverse_dns: resolved.reverse_dns.as_deref(),
            size: args.payload_size_bytes + 8,
            sent,
            received,
            loss_percent: loss,
            packets: lines_for_json,
        };
        println!("{}", serde_json::to_string_pretty(&json)?);
    }

    Ok(())
}

/// Packet-loss percentage; 0 sent counts as 0% loss (never NaN).
fn loss_percent(sent: u64, received: u64) -> f64 {
    if sent == 0 {
        return 0.0;
    }
    ((sent - received) as f64) * 100.0 / (sent as f64)
}

/// Replaces every case-insensitive, non-overlapping occurrence of
/// `needle_lower` (already lowercase) left to right. An empty needle returns
/// the input unchanged.
fn replace_ci(s: String, needle_lower: &str, replacement: &str) -> String {
    if needle_lower.is_empty() {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    let lower = s.to_ascii_lowercase();
    while let Some(pos) = lower[i..].find(needle_lower) {
        let abs = i + pos;
        out.push_str(&s[i..abs]);
        out.push_str(replacement);
        i = abs + needle_lower.len();
    }
    out.push_str(&s[i..]);
    out
}

/// Renders one packet line from a custom template: substitutes every known tag
/// case-insensitively and, when `timestamp_prefix` is set, prepends `timestamp`
/// unless the template already contains a %timestamp% tag. Unknown tags pass
/// through unchanged.
fn render_template_line(
    template: &str,
    resolved: &ResolvedTargetInfo,
    res: &PacketResult,
    payload_size_bytes: usize,
    timestamp_prefix: bool,
    timestamp: &str,
) -> String {
    let prefix_with_timestamp =
        timestamp_prefix && !template.to_ascii_lowercase().contains("%timestamp%");
    let ip_str = resolved.ip.to_string();
    let header_size = if resolved.ip.is_ipv4() {
        20 + 8
    } else {
        40 + 8
    };
    let mut out = template.to_string();
    let kv = [
        ("%host%", resolved.host.as_str()),
        ("%ip%", ip_str.as_str()),
        (
            "%reverse_dns%",
            resolved.reverse_dns.as_deref().unwrap_or(""),
        ),
        ("%icmp_seq%", &res.icmp_seq.to_string()),
        ("%time%", &format!("{:.2}", res.time_ms)),
        ("%timestamp%", timestamp),
        ("%size%", &(payload_size_bytes + header_size).to_string()),
        ("%size_no_headers%", &payload_size_bytes.to_string()),
        ("%error%", res.error.as_deref().unwrap_or("")),
    ];
    for (tag, val) in kv.iter() {
        out = replace_ci(out, tag.to_ascii_lowercase().as_str(), val);
    }
    if prefix_with_timestamp {
        format!("{} {}", timestamp, out)
    } else {
        out
    }
}

fn print_packet_line(args: &PingxArgs, resolved: &ResolvedTargetInfo, res: &PacketResult) {
    if args.quiet {
        return;
    }
    let ip_str = resolved.ip.to_string();
    let ts_val = chrono::Utc::now().to_rfc3339();
    let ts = if args.timestamp_prefix {
        format!("{} ", ts_val)
    } else {
        String::new()
    };
    match &args.output {
        OutputMode::Default => {
            if let Some(err) = &res.error {
                println!("{}error: {}", ts, err);
            } else {
                let from_name = if !args.numeric {
                    resolved.reverse_dns.as_deref().unwrap_or(&resolved.host)
                } else {
                    &resolved.host
                };
                println!(
                    "{}{} bytes from {} ({}): icmp_seq={} time={:.2} ms",
                    ts,
                    args.payload_size_bytes + 8,
                    from_name,
                    ip_str,
                    res.icmp_seq,
                    res.time_ms,
                );
            }
        }
        OutputMode::Csv => {
            // [timestamp,] host,ip,reverse_dns,size,icmp_seq,time
            let rdns = resolved.reverse_dns.as_deref().unwrap_or("");
            if args.timestamp_prefix {
                println!(
                    "{},{},{},{},{},{},{:.2}",
                    ts_val,
                    resolved.host,
                    ip_str,
                    rdns,
                    args.payload_size_bytes + 8,
                    res.icmp_seq,
                    res.time_ms,
                );
            } else {
                println!(
                    "{},{},{},{},{},{:.2}",
                    resolved.host,
                    ip_str,
                    rdns,
                    args.payload_size_bytes + 8,
                    res.icmp_seq,
                    res.time_ms,
                );
            }
        }
        OutputMode::Json => { /* aggregated at end */ }
        OutputMode::Template(tpl) => {
            println!(
                "{}",
                render_template_line(
                    tpl,
                    resolved,
                    res,
                    args.payload_size_bytes,
                    args.timestamp_prefix,
                    &ts_val,
                )
            );
        }
    }
}

fn print_stats(args: &PingxArgs, sent: u64, received: u64) {
    let loss = loss_percent(sent, received);
    match &args.output {
        OutputMode::Default => {
            println!("\n--- statistics ---");
            println!(
                "{} packets transmitted, {} received, {:.1}% packet loss",
                sent, received, loss
            );
        }
        OutputMode::Csv => {
            // Consistent, machine-readable stats line
            println!("stats,{},{},{:.1}", sent, received, loss);
        }
        OutputMode::Json | OutputMode::Template(_) => {
            // Suppress periodic human/template stats to avoid breaking format.
            // Final JSON includes stats; template stats are not defined yet.
        }
    }
}

fn rand_identifier() -> u16 {
    // Simple deterministic-ish identifier
    (std::process::id() as u16) ^ ((chrono::Utc::now().nanosecond() & 0xFFFF) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn resolved(ip: IpAddr, reverse_dns: Option<&str>) -> ResolvedTargetInfo {
        ResolvedTargetInfo {
            host: "example.com".to_string(),
            ip,
            reverse_dns: reverse_dns.map(str::to_string),
        }
    }

    fn packet(icmp_seq: u16, time_ms: f64, error: Option<&str>) -> PacketResult {
        PacketResult {
            icmp_seq,
            time_ms,
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn sanitize_display_passes_ordinary_hostnames_through() {
        assert_eq!(sanitize_display("host-1.example.com"), "host-1.example.com");
    }

    #[test]
    fn sanitize_display_escapes_control_and_bidi_characters() {
        assert_eq!(
            sanitize_display("evil\u{1b}[2J\u{202E}name"),
            "evil\\u{1b}[2J\\u{202e}name"
        );
    }

    #[test]
    fn loss_percent_with_zero_sent_is_zero() {
        assert_eq!(loss_percent(0, 0), 0.0);
    }

    #[test]
    fn loss_percent_with_no_replies_is_one_hundred() {
        assert_eq!(loss_percent(5, 0), 100.0);
    }

    #[test]
    fn loss_percent_with_partial_replies() {
        assert_eq!(loss_percent(4, 3), 25.0);
    }

    #[test]
    fn replace_ci_replaces_case_insensitively_and_keeps_surroundings() {
        assert_eq!(
            replace_ci("Host=%HOST% host=%Host%".to_string(), "%host%", "a"),
            "Host=a host=a"
        );
    }

    #[test]
    fn replace_ci_consumes_matches_left_to_right_without_overlap() {
        assert_eq!(replace_ci("aaa".to_string(), "aa", "b"), "ba");
    }

    #[test]
    fn replace_ci_with_empty_needle_returns_input_unchanged() {
        assert_eq!(replace_ci("abc".to_string(), "", "x"), "abc");
    }

    #[test]
    fn render_template_line_substitutes_every_tag() {
        let info = resolved(IpAddr::V4(Ipv4Addr::LOCALHOST), Some("localhost"));
        let res = packet(7, 1.234, Some("boom"));
        let line = render_template_line(
            "%host% %ip% %reverse_dns% %icmp_seq% %time% %timestamp% %size% %size_no_headers% %error%",
            &info,
            &res,
            56,
            false,
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(
            line,
            "example.com 127.0.0.1 localhost 7 1.23 2026-01-01T00:00:00Z 84 56 boom"
        );
    }

    #[test]
    fn render_template_line_uses_ipv6_header_sizes() {
        let info = resolved(IpAddr::V6(Ipv6Addr::LOCALHOST), None);
        let res = packet(1, 0.0, None);
        let line = render_template_line("%size%/%size_no_headers%", &info, &res, 56, false, "ts");
        assert_eq!(line, "104/56");
    }

    #[test]
    fn render_template_line_matches_tags_case_insensitively() {
        let info = resolved(IpAddr::V4(Ipv4Addr::LOCALHOST), None);
        let res = packet(3, 2.0, None);
        let line = render_template_line("%HOST% seq=%Icmp_Seq%", &info, &res, 56, false, "ts");
        assert_eq!(line, "example.com seq=3");
    }

    #[test]
    fn render_template_line_prefixes_timestamp_when_template_lacks_the_tag() {
        let info = resolved(IpAddr::V4(Ipv4Addr::LOCALHOST), None);
        let res = packet(3, 2.0, None);
        let line = render_template_line(
            "seq=%icmp_seq%",
            &info,
            &res,
            56,
            true,
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(line, "2026-01-01T00:00:00Z seq=3");
    }

    #[test]
    fn render_template_line_skips_prefix_when_template_has_timestamp_tag() {
        let info = resolved(IpAddr::V4(Ipv4Addr::LOCALHOST), None);
        let res = packet(3, 2.0, None);
        let line = render_template_line(
            "%TimeStamp% seq=%icmp_seq%",
            &info,
            &res,
            56,
            true,
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(line, "2026-01-01T00:00:00Z seq=3");
    }

    #[test]
    fn render_template_line_passes_unknown_tags_through() {
        let info = resolved(IpAddr::V4(Ipv4Addr::LOCALHOST), None);
        let res = packet(3, 2.0, None);
        let line = render_template_line("%host% %bogus%", &info, &res, 56, false, "ts");
        assert_eq!(line, "example.com %bogus%");
    }
}
