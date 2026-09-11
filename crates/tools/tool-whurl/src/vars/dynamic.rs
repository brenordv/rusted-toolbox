use std::collections::HashMap;
use std::process::Command;

use camino::Utf8Path;
use chrono::{Duration, Local, Utc};
use rand::RngExt;
use serde_json::Value;
use tracing::info;

use common_utils_ext::new_guid::new_guid;

use super::{VariableError, VariableMap};

/// Evaluation context threaded through dynamic-variable parsing: the shell
/// gate, the assignment-log toggle, and the per-run cache of `$shell` results.
/// One context lives for a whole `build_variables` pass, so a `$shell`
/// expression duplicated across layers executes once per run.
#[derive(Debug, Default)]
pub struct DynamicEvalContext {
    pub allow_shell: bool,
    pub log_assignments: bool,
    shell_cache: HashMap<String, String>,
}

impl DynamicEvalContext {
    pub fn new(allow_shell: bool, log_assignments: bool) -> Self {
        Self {
            allow_shell,
            log_assignments,
            shell_cache: HashMap::new(),
        }
    }
}

const DEFAULT_INT_MAX: i64 = i32::MAX as i64;
const LOCAL_DATE_FORMAT: &str = "%Y-%m-%d";
const LOCAL_TIME_FORMAT: &str = "%H:%M:%S";

const DESTRUCTIVE_COMMANDS: &[&str] = &[
    "apt",
    "apt-get",
    "attrib",
    "bash",
    "brew",
    "cargo",
    "cmd",
    "cfdisk",
    "chgrp",
    "chmod",
    "chsh",
    "cp",
    "cryptsetup",
    "curl",
    "dd",
    "del",
    "diskpart",
    "dnf",
    "docker",
    "erase",
    "fdisk",
    "fish",
    "flatpak",
    "format",
    "ftp",
    "gem",
    "getcap",
    "groupadd",
    "groupdel",
    "groupmod",
    "halt",
    "helm",
    "icacls",
    "ifconfig",
    "install",
    "ip",
    "ip6tables",
    "iptables",
    "kill",
    "killall",
    "kubectl",
    "ln",
    "losetup",
    "lua",
    "minikube",
    "mkfs",
    "mkfs.*",
    "mkswap",
    "mount",
    "mv",
    "nc",
    "ncat",
    "net localgroup",
    "net start",
    "net stop",
    "net user",
    "netcat",
    "nft",
    "nftables",
    "node",
    "npm",
    "pacman",
    "parted",
    "partprobe",
    "passwd",
    "perl",
    "php",
    "pip",
    "pip3",
    "pkill",
    "pnpm",
    "podman",
    "port",
    "poweroff",
    "powershell",
    "pwsh",
    "python",
    "python3",
    "rd",
    "reboot",
    "renice",
    "rm",
    "rmdir",
    "robocopy",
    "route",
    "rsync",
    "ruby",
    "sc",
    "scp",
    "service",
    "setcap",
    "setfacl",
    "setfattr",
    "sfdisk",
    "sftp",
    "sh",
    "shred",
    "shutdown",
    "snap",
    "socat",
    "ssh",
    "sysctl",
    "systemctl",
    "takeown",
    "trash",
    "trash-put",
    "tune2fs",
    "umount",
    "unlink",
    "useradd",
    "userdel",
    "usermod",
    "wget",
    "wipe",
    "xkill",
    "yarn",
    "yum",
    "zsh",
    "zypper",
];

pub fn parse_dynamic_variables_file(
    path: &Utf8Path,
    ctx: &mut DynamicEvalContext,
) -> Result<VariableMap, VariableError> {
    let contents = std::fs::read_to_string(path).map_err(|source| VariableError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    parse_dynamic_variables_from_str(path, &contents, ctx)
}

pub fn parse_dynamic_variables_from_str(
    path: &Utf8Path,
    contents: &str,
    ctx: &mut DynamicEvalContext,
) -> Result<VariableMap, VariableError> {
    let mut variables = VariableMap::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = raw_line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((raw_key, raw_value)) = trimmed.split_once('=') else {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "expected KEY=VALUE syntax".to_string(),
            });
        };

        let key = raw_key.trim();
        if key.is_empty() {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "variable name cannot be empty".to_string(),
            });
        }

        if key.contains(char::is_whitespace) {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "variable name cannot contain whitespace".to_string(),
            });
        }

        let expression = raw_value.trim();
        if expression.is_empty() {
            return Err(VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message: "variable value cannot be empty".to_string(),
            });
        }

        let (value, from_cache) =
            evaluate_expression(expression, ctx).map_err(|message| VariableError::Parse {
                path: path.to_path_buf(),
                line: line_number,
                message,
            })?;

        // The value (and the expression, which is command text) is
        // deliberately absent from this event: dynamic variables routinely
        // carry secrets, and the tracing stream can reach a log file.
        if ctx.log_assignments {
            info!(
                variable = %key,
                file = %path,
                line = line_number,
                cached = from_cache,
                "dynamic variable assigned"
            );
        }

        variables.insert(key.to_string(), value);
    }

    Ok(variables)
}

/// Evaluates one generator expression. The returned flag says whether the
/// value came from the per-run `$shell` cache.
fn evaluate_expression(
    expression: &str,
    ctx: &mut DynamicEvalContext,
) -> Result<(String, bool), String> {
    // Only $shell results are cached, keyed by the byte-identical expression:
    // caching $uuid or $int by expression would pin every occurrence to a
    // single value. Failures are never stored.
    if let Some(command) = parse_shell_command(expression)? {
        if let Some(cached) = ctx.shell_cache.get(expression) {
            return Ok((cached.clone(), true));
        }

        let value = execute_shell_command(&command, ctx.allow_shell)?;
        ctx.shell_cache
            .insert(expression.to_string(), value.clone());
        return Ok((value, false));
    }

    evaluate_generator(expression).map(|value| (value, false))
}

fn evaluate_generator(expression: &str) -> Result<String, String> {
    match expression {
        "$now" => Ok(Local::now().to_rfc3339()),
        "$utcnow" => Ok(Utc::now().to_rfc3339()),
        "$date" => Ok(Local::now().format(LOCAL_DATE_FORMAT).to_string()),
        "$utcdate" => Ok(Utc::now().format(LOCAL_DATE_FORMAT).to_string()),
        "$time" => Ok(Local::now().format(LOCAL_TIME_FORMAT).to_string()),
        "$utctime" => Ok(Utc::now().format(LOCAL_TIME_FORMAT).to_string()),
        "$uuid" => Ok(new_guid()),
        "$int" => Ok(rand::rng().random_range(0..=DEFAULT_INT_MAX).to_string()),
        "$float" => Ok(rand::rng().random_range(0.0..=1.0).to_string()),
        "$random[]" => Err("random generator requires at least one option".to_string()),
        _ => evaluate_complex_expression(expression),
    }
}

fn evaluate_complex_expression(expression: &str) -> Result<String, String> {
    if let Some(offset) = parse_offset(expression, "$date")? {
        return Ok((Local::now() + Duration::days(offset))
            .date_naive()
            .format(LOCAL_DATE_FORMAT)
            .to_string());
    }

    if let Some(offset) = parse_offset(expression, "$utcdate")? {
        return Ok((Utc::now() + Duration::days(offset))
            .date_naive()
            .format(LOCAL_DATE_FORMAT)
            .to_string());
    }

    if let Some(offset) = parse_offset(expression, "$time")? {
        return Ok((Local::now() + Duration::seconds(offset))
            .time()
            .format(LOCAL_TIME_FORMAT)
            .to_string());
    }

    if let Some(offset) = parse_offset(expression, "$utctime")? {
        return Ok((Utc::now() + Duration::seconds(offset))
            .time()
            .format(LOCAL_TIME_FORMAT)
            .to_string());
    }

    if let Some((min, max)) = parse_numeric_range(expression, "$int")? {
        return Ok(rand::rng().random_range(min..=max).to_string());
    }

    if let Some((min, max)) = parse_float_range(expression, "$float")? {
        return Ok(rand::rng().random_range(min..=max).to_string());
    }

    if let Some(options) = parse_random_options(expression)? {
        if options.is_empty() {
            return Err("random generator requires at least one option".to_string());
        }
        let index = rand::rng().random_range(0..options.len());
        return Ok(options[index].trim().to_string());
    }

    Err(format!("unsupported dynamic expression `{expression}`"))
}

fn parse_offset(expression: &str, prefix: &str) -> Result<Option<i64>, String> {
    if !expression.starts_with(prefix) {
        return Ok(None);
    }

    let suffix = &expression[prefix.len()..];
    if suffix.is_empty() {
        return Ok(Some(0));
    }

    if !suffix.starts_with('[') || !suffix.ends_with(']') {
        return Err(format!(
            "invalid offset syntax for `{prefix}`; expected `[+N]` or `[-N]`"
        ));
    }

    let inner = &suffix[1..suffix.len() - 1];
    let offset = inner.trim();
    if offset.is_empty() {
        return Err("offset value cannot be empty".to_string());
    }

    let value: i64 = offset
        .parse()
        .map_err(|_| format!("invalid offset `{offset}`; expected integer value"))?;
    Ok(Some(value))
}

fn parse_numeric_range(expression: &str, prefix: &str) -> Result<Option<(i64, i64)>, String> {
    if !expression.starts_with(prefix) {
        return Ok(None);
    }

    let start = prefix.len();
    if !expression[start..].starts_with('[') || !expression.ends_with(']') {
        return Err(format!(
            "invalid range syntax for `{prefix}`; expected `[min, max]`"
        ));
    }

    let inner = &expression[start + 1..expression.len() - 1];
    let (min, max) = parse_range::<i64>(inner)?;

    if min >= max {
        return Err("min must be less than max for int range".to_string());
    }

    Ok(Some((min, max)))
}

fn parse_float_range(expression: &str, prefix: &str) -> Result<Option<(f64, f64)>, String> {
    if !expression.starts_with(prefix) {
        return Ok(None);
    }

    let start = prefix.len();
    if !expression[start..].starts_with('[') || !expression.ends_with(']') {
        return Err(format!(
            "invalid range syntax for `{prefix}`; expected `[min, max]`"
        ));
    }

    let inner = &expression[start + 1..expression.len() - 1];
    let (min, max) = parse_range::<f64>(inner)?;

    if min >= max {
        return Err("min must be less than max for float range".to_string());
    }

    Ok(Some((min, max)))
}

fn parse_range<T>(input: &str) -> Result<(T, T), String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let mut parts = input.split(',');
    let Some(min_raw) = parts.next() else {
        return Err("range requires two values".to_string());
    };
    let Some(max_raw) = parts.next() else {
        return Err("range requires two values".to_string());
    };

    if parts.next().is_some() {
        return Err("range must contain exactly two values".to_string());
    }

    let min = min_raw
        .trim()
        .parse()
        .map_err(|err| format!("invalid minimum value `{min_raw}`: {err}"))?;
    let max = max_raw
        .trim()
        .parse()
        .map_err(|err| format!("invalid maximum value `{max_raw}`: {err}"))?;
    Ok((min, max))
}

fn parse_random_options(expression: &str) -> Result<Option<Vec<String>>, String> {
    if !expression.starts_with("$random[") || !expression.ends_with(']') {
        return Ok(None);
    }

    let inner = &expression["$random".len()..];
    if !inner.starts_with('[') {
        return Err("random options must be enclosed in brackets".to_string());
    }

    let slice = &inner[1..inner.len() - 1];
    let json_fragment = format!("[{slice}]");
    let value: Value = serde_json::from_str(&json_fragment)
        .map_err(|err| format!("failed to parse random options: {err}"))?;

    let array = value
        .as_array()
        .ok_or_else(|| "random options must be an array of double-quoted strings".to_string())?;

    let mut options = Vec::with_capacity(array.len());
    for entry in array {
        let Some(text) = entry.as_str() else {
            return Err("random options must be strings".to_string());
        };
        options.push(text.to_string());
    }

    Ok(Some(options))
}

fn parse_shell_command(expression: &str) -> Result<Option<String>, String> {
    if !expression.starts_with("$shell(") || !expression.ends_with(')') {
        return Ok(None);
    }

    let inner = &expression["$shell(".len()..expression.len() - 1];
    if inner.trim().is_empty() {
        return Err("shell command cannot be empty".to_string());
    }

    Ok(Some(inner.to_string()))
}

fn execute_shell_command(command: &str, allow_shell: bool) -> Result<String, String> {
    if !allow_shell {
        return Err(
            "shell dynamic variables are disabled; set WHURL_ALLOW_DYN_SHELL_VARS=true (or legacy WHURL_ALLOW_DYN_BASH_VARS=true) to enable"
                .to_string(),
        );
    }

    if is_destructive_command(command) {
        return Err(format!(
            "command `{command}` denied because it matches a restricted operation"
        ));
    }

    let output =
        spawn_shell(command).map_err(|err| format!("failed to execute shell command: {err}"))?;

    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string());
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "shell command exited with status {code}: {}",
            stderr.trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn spawn_shell(command: &str) -> std::io::Result<std::process::Output> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // cmd.exe parses its own command line and does not understand the
        // MSVCRT-style quote escaping std applies to regular args, so the
        // command must go through verbatim after /C or any quoted segment
        // (paths with spaces included) breaks.
        Command::new("cmd").raw_arg("/C").raw_arg(command).output()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        Command::new(shell).arg("-c").arg(command).output()
    }
}

fn is_destructive_command(command: &str) -> bool {
    let lowered = command.to_ascii_lowercase();
    let mut sanitized = lowered
        .replace(['\n', '\r', '\t'], " ")
        .replace("&&", " ")
        .replace("||", " ")
        .replace([';', '|', '&'], " ");

    sanitized = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");

    let tokens: Vec<String> = sanitized
        .split_whitespace()
        .map(|token| token.trim_matches(|ch| matches!(ch, '"' | '\'')))
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .split(['/', '\\'])
                .next_back()
                .unwrap_or(token)
                .to_string()
        })
        .collect();

    for pattern in DESTRUCTIVE_COMMANDS {
        if pattern.contains(' ') {
            if sanitized.contains(pattern) {
                return true;
            }
            continue;
        }

        if let Some(prefix) = pattern.strip_suffix(".*") {
            if tokens.iter().any(|token| token.starts_with(prefix)) {
                return true;
            }
            continue;
        }

        if tokens.iter().any(|token| token == pattern) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn path() -> Utf8PathBuf {
        Utf8PathBuf::from("inline.dvars")
    }

    #[test]
    fn parses_basic_generators() {
        let data = r#"
TOKEN=$uuid
DATE=$date
        "#;

        let mut ctx = DynamicEvalContext::default();
        let vars = parse_dynamic_variables_from_str(path().as_path(), data, &mut ctx)
            .expect("parse dynamic vars");

        assert!(vars.contains_key("TOKEN"));
        assert!(vars.contains_key("DATE"));
    }

    #[test]
    fn rejects_invalid_range() {
        let data = "BAD=$int[5, 2]";
        let mut ctx = DynamicEvalContext::default();
        let err = parse_dynamic_variables_from_str(path().as_path(), data, &mut ctx).unwrap_err();
        assert!(matches!(err, VariableError::Parse { .. }));
    }

    #[test]
    fn random_options_parses() {
        let options = parse_random_options(r#"$random["a", "b", "c"]"#)
            .expect("parse")
            .expect("options");
        assert_eq!(options.len(), 3);
    }

    #[test]
    fn date_offset_applies() {
        let value = evaluate_generator("$date[+1]").expect("evaluate date");
        let parsed =
            chrono::NaiveDate::parse_from_str(&value, LOCAL_DATE_FORMAT).expect("parse date");
        let expected = (Local::now() + Duration::days(1)).date_naive();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn int_range_is_respected() {
        for _ in 0..10 {
            let value = evaluate_generator("$int[-5, 5]")
                .expect("evaluate int")
                .parse::<i64>()
                .expect("parse int");
            assert!((-5..=5).contains(&value));
        }
    }

    #[test]
    fn shell_command_requires_permission() {
        let mut ctx = DynamicEvalContext::default();
        let err = evaluate_expression("$shell(echo hello)", &mut ctx).expect_err("shell disabled");
        assert!(err.contains("shell dynamic variables are disabled"));
    }

    #[test]
    fn shell_expressions_evaluate_once_per_run_across_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let counter = temp.path().join("count.txt");
        let command = format!(
            "$shell(echo x >> \"{}\" && echo token-1)",
            counter.display()
        );

        let layer_one = format!("token_a={command}");
        let layer_two = format!("token_b={command}");

        let mut ctx = DynamicEvalContext::new(true, false);
        let one = parse_dynamic_variables_from_str(
            Utf8PathBuf::from("one.dvars").as_path(),
            &layer_one,
            &mut ctx,
        )
        .expect("layer one");
        let two = parse_dynamic_variables_from_str(
            Utf8PathBuf::from("two.dvars").as_path(),
            &layer_two,
            &mut ctx,
        )
        .expect("layer two");

        assert_eq!(one.get("token_a"), two.get("token_b"));
        let executions = std::fs::read_to_string(&counter)
            .expect("counter file")
            .lines()
            .count();
        assert_eq!(executions, 1, "the shared expression must run exactly once");
    }

    #[test]
    fn distinct_shell_expressions_each_execute() {
        let mut ctx = DynamicEvalContext::new(true, false);
        let (a, from_cache_a) = evaluate_expression("$shell(echo a)", &mut ctx).expect("echo a");
        let (b, from_cache_b) = evaluate_expression("$shell(echo b)", &mut ctx).expect("echo b");

        assert_eq!(a, "a");
        assert_eq!(b, "b");
        assert!(!from_cache_a);
        assert!(!from_cache_b);

        let (again, from_cache) =
            evaluate_expression("$shell(echo a)", &mut ctx).expect("echo a again");
        assert_eq!(again, "a");
        assert!(from_cache);
    }

    #[test]
    fn non_shell_generators_are_not_cached() {
        let data = "ID1=$uuid\nID2=$uuid\n";
        let mut ctx = DynamicEvalContext::default();
        let vars = parse_dynamic_variables_from_str(path().as_path(), data, &mut ctx)
            .expect("parse dynamic vars");

        assert_ne!(vars.get("ID1"), vars.get("ID2"));
    }

    #[test]
    fn detects_destructive_commands() {
        assert!(is_destructive_command("rm -rf /"));
        assert!(is_destructive_command("mkfs.ext4 /dev/sda1"));
        assert!(is_destructive_command("net stop Spooler"));
        assert!(!is_destructive_command("echo hello world"));
    }
}
