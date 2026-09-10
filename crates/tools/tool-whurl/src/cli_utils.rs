use crate::models::{Cli, Command, DryRunArgs, ExecutionArgs, KeyValue, ListArgs, RunArgs};
use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::header_format::format_config_item;
use common_cli::tool_log_level::ToolLogLevel;

/// Wrapper for Hurl with a few additional features.
///
/// This tool enables including one hurl file into another, and chaining their execution.
#[derive(Parser, Debug)]
#[command(about, long_about, version, arg_required_else_help = true)]
struct CliArgs {
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// List available APIs or requests
    List {
        /// API name to inspect. When omitted, prints all APIs.
        #[arg(value_name = "API", num_args = 0..=1)]
        api: Option<String>,
    },
    /// Expand includes and execute a request collection
    Run(RunCliArgs),
    /// Expand includes and display the merged Hurl document
    #[command(name = "dry-run")]
    DryRun(DryRunCliArgs),
}

#[derive(Args, Debug)]
struct ExecutionCliArgs {
    /// API directory containing the Hurl file.
    #[arg(value_name = "API")]
    api: String,

    /// Name of the Hurl file to execute (extension optional, relative to the API directory).
    #[arg(value_name = "FILE")]
    file: String,

    /// Named environment to load from the API's vars directory.
    #[arg(long = "env", value_name = "NAME")]
    env: Option<String>,

    /// Path to an additional variables file (key=value pairs).
    #[arg(long = "vars-file", value_name = "PATH", value_parser = clap::value_parser!(Utf8PathBuf))]
    vars_file: Option<Utf8PathBuf>,

    /// Provide an inline variable assignment (can be repeated).
    #[arg(long = "var", value_name = "KEY=VALUE", value_parser = parse_key_value)]
    var: Vec<KeyValue>,

    /// Override the root directory for resolving file, responses, and captures.
    #[arg(long = "file-root", value_name = "PATH", value_parser = clap::value_parser!(Utf8PathBuf))]
    file_root: Option<Utf8PathBuf>,

    /// Increase output verbosity. Pass twice for extra detail.
    #[arg(long = "verbose", short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(flatten)]
    common: CommonToolArgsNoVerbose,
}

#[derive(Args, Debug)]
struct RunCliArgs {
    #[command(flatten)]
    exec: ExecutionCliArgs,

    /// Write execution JSON report to the given path.
    #[arg(long = "json", value_name = "PATH", value_parser = clap::value_parser!(Utf8PathBuf))]
    json: Option<Utf8PathBuf>,

    /// Emit a concise test-style summary after execution.
    #[arg(long = "test")]
    test: bool,

    /// Suppress header/log output and print only the final result object as JSON.
    #[arg(long = "print-only-full-response")]
    print_only_full_response: bool,

    /// Suppress header/log output and print only the last response body.
    #[arg(long = "print-only-response-body")]
    print_only_response_body: bool,

    /// Disable header/log output (behaves similar to legacy mode).
    #[arg(long = "silent")]
    silent: bool,
}

impl RunCliArgs {
    /// True when any of the output-suppressing modes is active.
    fn silent_mode(&self) -> bool {
        self.silent || self.print_only_full_response || self.print_only_response_body
    }
}

#[derive(Args, Debug)]
struct DryRunCliArgs {
    #[command(flatten)]
    exec: ExecutionCliArgs,

    /// Print boundary markers between includes.
    #[arg(
        long = "show-boundaries",
        value_name = "BOOL",
        num_args = 0..=1,
        default_value = "true",
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    show_boundaries: bool,
}

/// Parses command-line arguments, boots logging and the optional `--app-header`
/// block, and returns the runtime configuration.
pub fn initialize() -> Cli {
    let args = CliArgs::parse();

    boot(&args);

    to_cli(args)
}

/// Initializes logging and prints the `--app-header` block for the parsed
/// command, before any command work runs, so every later tracing call has a
/// subscriber. In the silent modes the header stays suppressed even when
/// `--app-header` is set: those flags promise header-free output.
fn boot(args: &CliArgs) {
    match &args.command {
        CliCommand::List { .. } => {
            // `list` exposes no logging flags; the all-off defaults still log
            // at Info to stderr through the same boot path.
            CommonToolArgsNoVerbose::default().app_boot_up_with_level(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                ToolLogLevel::Info,
                0,
                false,
                None::<fn()>,
            );
        }
        CliCommand::Run(run) => boot_exec(
            &run.exec,
            resolve_log_level(&args.command),
            run_header_visible(run),
            run_header_lines(run),
        ),
        CliCommand::DryRun(dry_run) => boot_exec(
            &dry_run.exec,
            resolve_log_level(&args.command),
            dry_run.exec.common.app_header,
            dry_run_header_lines(dry_run),
        ),
    }
}

fn boot_exec(
    exec: &ExecutionCliArgs,
    derived_level: ToolLogLevel,
    show_header: bool,
    header_lines: Vec<String>,
) {
    let mut common = exec.common.clone();
    common.app_header = show_header;

    common.app_boot_up_with_level(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        derived_level,
        exec.verbose,
        false,
        Some(move || {
            for line in &header_lines {
                println!("{line}");
            }
        }),
    );
}

/// The tracing level whurl derives when `--log-level` is not passed: Error in
/// the run silent modes so only failures surface, Info everywhere else. An
/// explicit `--log-level` overrides the derivation (resolved inside the boot).
fn resolve_log_level(command: &CliCommand) -> ToolLogLevel {
    match command {
        CliCommand::Run(run) if run.silent_mode() => ToolLogLevel::Error,
        _ => ToolLogLevel::Info,
    }
}

/// Whether the run header may print: `--app-header` opts in, and any silent
/// mode suppresses it.
fn run_header_visible(run: &RunCliArgs) -> bool {
    run.exec.common.app_header && !run.silent_mode()
}

/// The header lines shared by `run` and `dry-run`: the two positional
/// arguments as given (path resolution has not happened at boot time), then
/// each optional input that was set. Inline variables list their keys only;
/// values never reach the header.
fn exec_header_lines(exec: &ExecutionCliArgs) -> Vec<String> {
    let mut lines = vec![
        format_config_item("API", &exec.api),
        format_config_item("Request", &exec.file),
    ];

    if let Some(env) = exec.env.as_ref() {
        lines.push(format_config_item("Environment", env));
    }

    if let Some(vars_file) = exec.vars_file.as_ref() {
        lines.push(format_config_item("Vars File", vars_file));
    }

    if !exec.var.is_empty() {
        let listed = exec
            .var
            .iter()
            .map(|kv| kv.key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format_config_item("Inline Vars", listed));
    }

    if let Some(file_root) = exec.file_root.as_ref() {
        lines.push(format_config_item("File Root", file_root));
    }

    lines
}

fn run_header_lines(run: &RunCliArgs) -> Vec<String> {
    let mut lines = exec_header_lines(&run.exec);

    if let Some(json_output) = run.json.as_ref() {
        lines.push(format_config_item("JSON Output", json_output));
    }

    if run.test {
        lines.push(format_config_item("Test Mode", "enabled"));
    }

    lines
}

fn dry_run_header_lines(dry_run: &DryRunCliArgs) -> Vec<String> {
    let mut lines = exec_header_lines(&dry_run.exec);
    lines.push(format_config_item(
        "Show Boundaries",
        dry_run.show_boundaries,
    ));
    lines
}

/// Maps the parsed derive-based arguments onto the tool's runtime model.
fn to_cli(args: CliArgs) -> Cli {
    let command = match args.command {
        CliCommand::List { api } => Command::List(ListArgs { api }),
        CliCommand::Run(run) => Command::Run(RunArgs {
            exec: to_execution_args(run.exec),
            json_output: run.json,
            test_mode: run.test,
            print_only_full_response: run.print_only_full_response,
            print_only_response_body: run.print_only_response_body,
            silent: run.silent,
        }),
        CliCommand::DryRun(dry_run) => Command::DryRun(DryRunArgs {
            exec: to_execution_args(dry_run.exec),
            show_boundaries: dry_run.show_boundaries,
        }),
    };

    Cli { command }
}

fn to_execution_args(exec: ExecutionCliArgs) -> ExecutionArgs {
    ExecutionArgs {
        api: exec.api,
        file: exec.file,
        env: exec.env,
        vars_file: exec.vars_file,
        inline_vars: exec.var,
        file_root: exec.file_root,
        verbosity: exec.verbose,
    }
}

fn parse_key_value(raw: &str) -> Result<KeyValue, String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err("expected KEY=VALUE".to_string());
    };

    let key = key.trim().to_string();
    let value = value.to_string();

    if key.is_empty() {
        return Err("variable name cannot be empty".to_string());
    }

    if key.contains(char::is_whitespace) {
        return Err("variable name cannot contain whitespace".to_string());
    }

    Ok(KeyValue { key, value })
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
    fn run_subcommand_maps_execution_args() {
        let args = CliArgs::try_parse_from([
            "whurl",
            "run",
            "my-api",
            "login",
            "--env",
            "staging",
            "--var",
            "token=abc",
            "-vv",
        ])
        .unwrap();
        let cli = to_cli(args);

        match cli.command {
            Command::Run(run) => {
                assert_eq!(run.exec.api, "my-api");
                assert_eq!(run.exec.file, "login");
                assert_eq!(run.exec.env.as_deref(), Some("staging"));
                assert_eq!(run.exec.verbosity, 2);
                assert_eq!(run.exec.inline_vars.len(), 1);
                assert_eq!(run.exec.inline_vars[0].key, "token");
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn dry_run_show_boundaries_defaults_to_true() {
        let args = CliArgs::try_parse_from(["whurl", "dry-run", "my-api", "login"]).unwrap();
        let cli = to_cli(args);
        match cli.command {
            Command::DryRun(dry_run) => assert!(dry_run.show_boundaries),
            other => panic!("expected dry-run command, got {other:?}"),
        }
    }

    #[test]
    fn inline_var_rejects_missing_equals() {
        assert!(CliArgs::try_parse_from(["whurl", "run", "api", "file", "--var", "bad"]).is_err());
    }

    fn parse_run(argv: &[&str]) -> RunCliArgs {
        let args = CliArgs::try_parse_from(argv).unwrap();
        match args.command {
            CliCommand::Run(run) => run,
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_parses_common_flags_alongside_owned_verbose_count() {
        let run = parse_run(&[
            "whurl",
            "run",
            "api",
            "file",
            "-vv",
            "--log-level",
            "debug",
            "--app-header",
        ]);

        assert_eq!(run.exec.verbose, 2);
        assert_eq!(run.exec.common.log_level, Some(ToolLogLevel::Debug));
        assert!(run.exec.common.app_header);
    }

    #[test]
    fn resolve_log_level_derives_error_only_in_run_silent_modes() {
        let level = |argv: &[&str]| {
            let args = CliArgs::try_parse_from(argv).unwrap();
            resolve_log_level(&args.command)
        };

        assert_eq!(level(&["whurl", "run", "a", "f"]), ToolLogLevel::Info);
        assert_eq!(
            level(&["whurl", "run", "a", "f", "--silent"]),
            ToolLogLevel::Error
        );
        assert_eq!(
            level(&["whurl", "run", "a", "f", "--print-only-full-response"]),
            ToolLogLevel::Error
        );
        assert_eq!(
            level(&["whurl", "run", "a", "f", "--print-only-response-body"]),
            ToolLogLevel::Error
        );
        assert_eq!(level(&["whurl", "dry-run", "a", "f"]), ToolLogLevel::Info);
        assert_eq!(level(&["whurl", "list"]), ToolLogLevel::Info);
    }

    #[test]
    fn explicit_log_level_overrides_the_derived_error() {
        let run = parse_run(&["whurl", "run", "a", "f", "--silent", "--log-level", "info"]);

        let derived = ToolLogLevel::Error;
        assert_eq!(run.exec.common.resolved_level(derived), ToolLogLevel::Info);
    }

    #[test]
    fn header_suppressed_in_silent_modes_even_with_app_header() {
        for silent_flag in [
            "--silent",
            "--print-only-full-response",
            "--print-only-response-body",
        ] {
            let run = parse_run(&["whurl", "run", "a", "f", "--app-header", silent_flag]);
            assert!(
                !run_header_visible(&run),
                "header must stay hidden under {silent_flag}"
            );
        }

        let plain = parse_run(&["whurl", "run", "a", "f", "--app-header"]);
        assert!(run_header_visible(&plain));

        let opted_out = parse_run(&["whurl", "run", "a", "f"]);
        assert!(!run_header_visible(&opted_out));
    }

    #[test]
    fn run_header_lines_include_only_set_items() {
        let minimal = run_header_lines(&parse_run(&["whurl", "run", "api", "file"]));
        assert_eq!(
            minimal,
            vec![
                format_config_item("API", "api"),
                format_config_item("Request", "file"),
            ]
        );

        let full = run_header_lines(&parse_run(&[
            "whurl",
            "run",
            "api",
            "file",
            "--env",
            "staging",
            "--vars-file",
            "extra.vars",
            "--var",
            "token=abc",
            "--file-root",
            "root",
            "--json",
            "out.json",
            "--test",
        ]));

        assert!(full.contains(&format_config_item("Environment", "staging")));
        assert!(full.contains(&format_config_item("Vars File", "extra.vars")));
        assert!(full.contains(&format_config_item("Inline Vars", "token")));
        assert!(full.contains(&format_config_item("File Root", "root")));
        assert!(full.contains(&format_config_item("JSON Output", "out.json")));
        assert!(full.contains(&format_config_item("Test Mode", "enabled")));
        assert!(
            full.iter().all(|line| !line.contains("abc")),
            "inline variable values must never reach the header"
        );
    }

    #[test]
    fn dry_run_header_lines_always_include_show_boundaries() {
        let args = CliArgs::try_parse_from(["whurl", "dry-run", "api", "file"]).unwrap();
        let dry_run = match args.command {
            CliCommand::DryRun(dry_run) => dry_run,
            other => panic!("expected dry-run command, got {other:?}"),
        };

        let lines = dry_run_header_lines(&dry_run);
        assert!(lines.contains(&format_config_item("Show Boundaries", true)));
    }
}
