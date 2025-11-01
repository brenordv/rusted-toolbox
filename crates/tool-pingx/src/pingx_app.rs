use crate::cli_utils::print_header;
use crate::models::{IpMode, OutputMode, PacketResult, PingxArgs, ResolvedTargetInfo};
use anyhow::Result;
use chrono::Timelike;
use dns_lookup::lookup_addr;
use serde::Serialize;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use std::net::{IpAddr, ToSocketAddrs};
use surge_ping::{Client, ConfigBuilder, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use tokio::time::{sleep, Duration, Instant};

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
    icmp_seq: u64,
    time: f64,
}

pub fn resolve_target(args: &PingxArgs) -> Result<ResolvedTargetInfo> {
    let host = args.target.clone();

    // Try to resolve using ToSocketAddrs to respect system resolver
    // Default port 0 just for resolution
    let mut addrs: Vec<IpAddr> = Vec::new();
    if let Ok(iter) = (host.as_str(), 0).to_socket_addrs() {
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
    } else if let Ok(ip) = host.parse::<IpAddr>() {
        // Fallback: direct parse
        let is_ok = match args.ip_mode {
            IpMode::Auto => true,
            IpMode::V4 => ip.is_ipv4(),
            IpMode::V6 => ip.is_ipv6(),
        };
        if is_ok {
            addrs.push(ip);
        }
    }

    let ip = *addrs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve target"))?;

    let reverse_dns = if args.numeric {
        None
    } else {
        lookup_addr(&ip).ok()
    };

    Ok(ResolvedTargetInfo {
        host,
        ip,
        reverse_dns,
    })
}

pub async fn run_ping(args: &PingxArgs) -> Result<()> {
    let resolved = resolve_target(args)?;
    print_header(args, &resolved);

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

        let timeout = Duration::from_secs_f64(args.per_reply_timeout_secs);

        let mut pinger = client.pinger(resolved.ip, identifier).await;

        pinger.timeout(timeout);

        let payload = vec![0u8; args.payload_size_bytes];
        let mut had_error = false;
        match pinger.ping(PingSequence(sequence as u16), &payload).await {
            Ok((IcmpPacket::V4(_packet), dur)) => {
                received += 1;
                let time_ms = dur.as_secs_f64() * 1000.0;
                let packet_res = PacketResult {
                    icmp_seq: sequence,
                    time_ms,
                    error: None,
                };
                print_packet_line(args, &resolved, &packet_res);
                if matches!(args.output, OutputMode::Json) {
                    lines_for_json.push(PacketLine {
                        icmp_seq: sequence,
                        time: time_ms,
                    });
                }
            }
            Ok((IcmpPacket::V6(_packet), dur)) => {
                received += 1;
                let time_ms = dur.as_secs_f64() * 1000.0;
                let packet_res = PacketResult {
                    icmp_seq: sequence,
                    time_ms,
                    error: None,
                };
                print_packet_line(args, &resolved, &packet_res);
                if matches!(args.output, OutputMode::Json) {
                    lines_for_json.push(PacketLine {
                        icmp_seq: sequence,
                        time: time_ms,
                    });
                }
            }
            Err(e) => {
                had_error = true;
                let packet_res = PacketResult {
                    icmp_seq: sequence,
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
        let loss = if sent == 0 {
            0.0
        } else {
            ((sent - received) as f64) * 100.0 / (sent as f64)
        };
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

fn replace_ci(s: String, needle_lower: &str, replacement: &str) -> String {
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

fn print_packet_line(args: &PingxArgs, resolved: &ResolvedTargetInfo, res: &PacketResult) {
    if args.quiet {
        return;
    }
    let ip_str = resolved.ip.to_string();
    let mut ts_prefix = args.timestamp_prefix;
    let ts_val = chrono::Utc::now().to_rfc3339();
    let ts = if ts_prefix {
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
            // Avoid double timestamp prefix if template contains %timestamp%
            let tpl_lower = tpl.to_ascii_lowercase();
            if tpl_lower.contains("%timestamp%") {
                ts_prefix = false;
            }
            let mut out = tpl.clone();
            let header_size = if resolved.ip.is_ipv4() {
                20 + 8
            } else {
                40 + 8
            };
            let kv = [
                ("%host%", resolved.host.as_str()),
                ("%ip%", ip_str.as_str()),
                (
                    "%reverse_dns%",
                    resolved.reverse_dns.as_deref().unwrap_or(""),
                ),
                ("%icmp_seq%", &res.icmp_seq.to_string()),
                ("%time%", &format!("{:.2}", res.time_ms)),
                ("%timestamp%", &ts_val),
                (
                    "%size%",
                    &(args.payload_size_bytes + header_size).to_string(),
                ),
                ("%size_no_headers%", &args.payload_size_bytes.to_string()),
                ("%error%", res.error.as_deref().unwrap_or("")),
            ];
            for (tag, val) in kv.iter() {
                out = replace_ci(out, tag.to_ascii_lowercase().as_str(), val);
            }
            let ts2 = if ts_prefix {
                format!("{} ", ts_val)
            } else {
                String::new()
            };
            println!("{}{}", ts2, out);
        }
    }
}

fn print_stats(args: &PingxArgs, sent: u64, received: u64) {
    let loss = if sent == 0 {
        0.0
    } else {
        ((sent - received) as f64) * 100.0 / (sent as f64)
    };
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
