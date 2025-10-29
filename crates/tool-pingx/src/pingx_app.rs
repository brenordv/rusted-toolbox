use std::net::{IpAddr, ToSocketAddrs};
use crate::cli_utils::{print_header};
use crate::models::{IpMode, OutputMode, PacketResult, PingxArgs, ResolvedTargetInfo};
use anyhow::Result;
use chrono::Timelike;
use dns_lookup::lookup_addr;
use serde::Serialize;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use surge_ping::{Client, ConfigBuilder, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use tokio::time::{sleep, Duration, Instant};

#[derive(Serialize)]
struct JsonOutput<'a> {
    host: &'a str,
    ip: String,
    reverse_dns: Option<&'a str>,
    size: usize,
    packets: Vec<PacketLine>,
}

#[derive(Serialize)]
struct PacketLine {
    icmp_seq: u64,
    ttl: Option<u8>,
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
                IpMode::V4 => if ip.is_ipv4() { addrs.push(ip) },
                IpMode::V6 => if ip.is_ipv6() { addrs.push(ip) },
            }
        }
    } else if let Ok(ip) = host.parse::<IpAddr>() {
        // Fallback: direct parse
        let is_ok = match args.ip_mode {
            IpMode::Auto => true,
            IpMode::V4 => ip.is_ipv4(),
            IpMode::V6 => ip.is_ipv6(),
        };
        if is_ok { addrs.push(ip); }
    }

    let ip = *addrs.first().ok_or_else(|| anyhow::anyhow!("Failed to resolve target"))?;

    let reverse_dns = if args.numeric { None } else { lookup_addr(&ip).ok() };

    Ok(ResolvedTargetInfo { host, ip, reverse_dns })
}

pub async fn run_ping(args: &PingxArgs) -> Result<()> {
    let resolved = resolve_target(args)?;
    print_header(args, &resolved);

    let shutdown = setup_graceful_shutdown(false);

    let mut config_builder = ConfigBuilder::default();

    if let Some(ttl) = args.ttl {
        config_builder = config_builder.ttl(ttl as u32);
    }

    let icmp = if resolved.ip.is_ipv4() { ICMP::V4 } else { ICMP::V6 };
    config_builder = config_builder.kind(icmp);

    let config = config_builder.build();

    let client = Client::new(&config)?;
    let mut sequence: u64 = 0;
    let identifier = PingIdentifier(rand_identifier());
    let deadline_start = Instant::now();
    let mut sent: u64 = 0;
    let mut received: u64 = 0;
    let mut lines_for_json: Vec<PacketLine> = Vec::new();

    loop {
        if let Some(deadline) = args.overall_deadline_secs {
            if deadline_start.elapsed() >= Duration::from_secs_f64(deadline) {
                break;
            }
        }

        if !args.is_infinite() && sent >= args.count as u64 {
            break;
        }

        if shutdown.load(std::sync::atomic::Ordering::Relaxed) { break; }

        sequence += 1;
        sent += 1;

        let start = Instant::now();
        let timeout = Duration::from_secs_f64(args.per_reply_timeout_secs);

        let mut pinger = client.pinger(resolved.ip, identifier).await;

        pinger.timeout(timeout);

        let payload = vec![0u8; args.payload_size_bytes];
        match pinger.ping(PingSequence(sequence as u16), &payload).await {
            Ok((IcmpPacket::V4(_packet), dur)) => {
                received += 1;
                let time_ms = dur.as_secs_f64() * 1000.0;
                let packet_res = PacketResult { icmp_seq: sequence, ttl: None, time_ms, error: None };
                print_packet_line(args, &resolved.host, &resolved.ip.to_string(), &packet_res);
                if matches!(args.output, OutputMode::Json) { lines_for_json.push(PacketLine { icmp_seq: sequence, ttl: None, time: time_ms }); }
            }
            Ok((IcmpPacket::V6(_packet), dur)) => {
                received += 1;
                let time_ms = dur.as_secs_f64() * 1000.0;
                let packet_res = PacketResult { icmp_seq: sequence, ttl: None, time_ms, error: None };
                print_packet_line(args, &resolved.host, &resolved.ip.to_string(), &packet_res);
                if matches!(args.output, OutputMode::Json) { lines_for_json.push(PacketLine { icmp_seq: sequence, ttl: None, time: time_ms }); }
            }
            Err(e) => {
                let packet_res = PacketResult { icmp_seq: sequence, ttl: None, time_ms: 0.0, error: Some(e.to_string()) };
                print_packet_line(args, &resolved.host, &resolved.ip.to_string(), &packet_res);
                if args.beep_on_loss { print!("\x07"); }
            }
        }

        if let Some(every) = args.stats_every_secs {
            if (start.elapsed().as_secs_f64() % every) < args.interval_secs.min(0.5) {
                print_stats(args, sent, received);
            }
        }

        sleep(Duration::from_secs_f64(args.interval_secs)).await;
    }

    // Final stats
    print_stats(args, sent, received);

    if matches!(args.output, OutputMode::Json) {
        let json = JsonOutput {
            host: &resolved.host,
            ip: resolved.ip.to_string(),
            reverse_dns: resolved.reverse_dns.as_deref(),
            size: args.payload_size_bytes + 8,
            packets: lines_for_json,
        };
        println!("{}", serde_json::to_string_pretty(&json)?);
    }

    Ok(())
}

fn print_packet_line(args: &PingxArgs, host: &str, ip: &str, res: &PacketResult) {
    if args.quiet { return; }
    let ts = if args.timestamp_prefix { format!("{} ", chrono::Utc::now().to_rfc3339()) } else { String::new() };
    match &args.output {
        OutputMode::Default => {
            if let Some(err) = &res.error {
                println!("{}error: {}", ts, err);
            } else {
                println!(
                    "{}{} bytes from {} ({}): icmp_seq={} ttl={} time={:.2} ms",
                    ts,
                    args.payload_size_bytes + 8,
                    host,
                    ip,
                    res.icmp_seq,
                    res.ttl.unwrap_or(0),
                    res.time_ms,
                );
            }
        }
        OutputMode::Csv => {
            // host,ip,reverse_dns,size,icmp_seq,time,ttl
            println!(
                "{},{},,{}, {},{:.2},{}",
                host,
                ip,
                args.payload_size_bytes + 8,
                res.icmp_seq,
                res.time_ms,
                res.ttl.unwrap_or(0),
            );
        }
        OutputMode::Json => { /* aggregated at end */ }
        OutputMode::Template(tpl) => {
            let mut out = tpl.clone();
            out = out.replace("%host%", host).replace("%HOST%", host);
            out = out.replace("%ip%", ip).replace("%IP%", ip);
            out = out.replace("%icmp_seq%", &res.icmp_seq.to_string());
            out = out.replace("%ttl%", &res.ttl.unwrap_or(0).to_string());
            out = out.replace("%time%", &format!("{:.2}", res.time_ms));
            out = out.replace("%timestamp%", &chrono::Utc::now().to_rfc3339());
            let header_size = if ip.contains(':') { 40 + 8 } else { 20 + 8 };
            out = out.replace("%size%", &(args.payload_size_bytes + header_size).to_string());
            out = out.replace("%size_no_headers%", &(args.payload_size_bytes + 8).to_string());
            if let Some(err) = &res.error { 
                out = out.replace("%error%", err); 
            } else {
                out = out.replace("%error%", "");
            }
            println!("{}{}", ts, out);
        }
    }
}

fn print_stats(args: &PingxArgs, sent: u64, received: u64) {
    let loss = if sent == 0 { 0.0 } else { ((sent - received) as f64) * 100.0 / (sent as f64) };
    println!("\n--- statistics ---");
    println!("{} packets transmitted, {} received, {:.1}% packet loss", sent, received, loss);
}

fn rand_identifier() -> u16 {
    // Simple deterministic-ish identifier
    (std::process::id() as u16) ^ ((chrono::Utc::now().nanosecond() & 0xFFFF) as u16)
}