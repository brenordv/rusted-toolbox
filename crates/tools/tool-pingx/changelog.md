# Changelog

## 2.0.3
- Internal dedup, output bytes unchanged: reverse-DNS display escaping now comes from
  `common_utils::string_utils::escape_for_terminal_display` (the local `sanitize_display` copy is
  gone), and the `Resolved Target` / `Continuous mode` header lines render through
  `common_cli::header_format::format_config_label` instead of hand-assembled constants.

## 2.0.2
- The reverse-DNS lookup (`dns_lookup::lookup_addr`, a synchronous `getnameinfo` wrapper) now runs
  on tokio's blocking pool via `tokio::task::spawn_blocking` instead of stalling the async runtime.
  A panicked lookup task logs at debug level; an ordinary lookup miss stays silent as before.
- The resolved reverse-DNS name is sanitized before display: control characters (C0, DEL, C1) and
  Unicode bidirectional-control characters render as escape sequences. PTR records are
  attacker-controlled and the name flows into default, CSV, and template output.
- Readme: `--log-level` has no `-L` short (the shared flag is long-only); the readme line and the
  2.0.0 entry below are corrected. The CSV sample's empty reverse_dns field no longer shows a
  stray space.

## 2.0.1
- Forward DNS resolution and the retry delay no longer block the async runtime: `resolve_target`
  is now async, resolves DNS through `tokio::net::lookup_host`, and awaits `tokio::time::sleep`
  between retries instead of calling `std::thread::sleep` on a tokio worker. The reverse-DNS
  lookup (`dns_lookup::lookup_addr`) is still a synchronous call.
- All outputs (default, CSV, JSON, template) now report the on-wire 16-bit `icmp_seq`, so the
  displayed number keeps matching the transmitted one after 65535 packets.
- The resolved-target header line names what each size number includes:
  `Payload size: N (with IP+ICMP headers: M)` instead of `Packet Size: N (with headers: M)`.
  The numbers are unchanged, as are the per-packet, CSV, and JSON sizes (payload + 8).
- The header config lines are rendered through the shared `common_cli::header_format` helpers;
  output bytes are unchanged apart from the size relabel above.
- Extracted pure helpers for testability: `loss_percent` (0 sent yields 0%, never NaN) and
  `render_template_line` (timestamp injected by the caller). Unit tests cover them plus
  `replace_ci`, `template_has_any_tag`, and `PingxArgs::is_infinite`.
- `replace_ci` returns the input unchanged for an empty pattern; it previously looped forever on
  that input (unreachable through the CLI, which only passes fixed tag names).
- Fixed the `ipv4_and_ipv6_are_mutually_exclusive` unit test: it unwrapped a parse that clap
  already rejects through `conflicts_with`, so it always panicked; it now asserts that parse-time
  rejection.
- Readme corrections: removed the nonexistent `-m/--compact-header` and `-p/--no-header` flags
  and the compact-header example, dropped the adjustable-TTL claim, and replaced the outdated
  "XPing v1.0.0" header sample with the actual output (the `pingx (2.0.1)` standard block only
  appears under `--app-header`).

## 2.0.0
- Refactored to fit the new tooling.
- Rewrote CLI parsing with the `clap` derive API and the shared `CommonToolArgs`, replacing the
  hand-built `Command` from the retired `shared` crate.
- Verbose output now uses the shared `--verbose` flag. The old `-v` short alias is gone.
- Gained the shared runtime flags: `--log-level` (case-insensitive), `--app-header`,
  `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Logging is now installed through the shared `AppLogger` during boot-up instead of a standalone
  initializer.
- Exit codes are explicit: `0` on success and `1` on failure.

## 1.0.0 🎃
Initial release