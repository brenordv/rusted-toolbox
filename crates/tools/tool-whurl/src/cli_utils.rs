use crate::files::ResolvedRunContext;
use crate::models::{Cli, Command, DryRunArgs, ExecutionArgs, KeyValue, ListArgs, RunArgs};
use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use common_utils::constants::DASH_LINE;

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

/// Parses command-line arguments and returns the runtime configuration.
pub fn initialize() -> Cli {
    let args = CliArgs::parse();

    to_cli(args)
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

pub fn print_runtime_info(context: &ResolvedRunContext, args: &RunArgs) {
    println!(
        "{} v{}",
        env!("CARGO_PKG_NAME").to_uppercase(),
        env!("CARGO_PKG_VERSION")
    );
    println!("{DASH_LINE}");
    println!("- API: {}", context.resolution.api);
    println!("- Request: {}", context.display_path);

    if let Some(env_name) = args.exec.env.as_ref() {
        println!("- Environment: {env_name}");
    }

    if let Some(vars_file) = args.exec.vars_file.as_ref() {
        println!("- Vars File: {}", vars_file);
    }

    if !args.exec.inline_vars.is_empty() {
        let listed = args
            .exec
            .inline_vars
            .iter()
            .map(|kv| kv.key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("- Inline Vars: {listed}");
    }

    if let Some(file_root) = args.exec.file_root.as_ref() {
        println!("- File Root: {}", file_root);
    }

    if let Some(json_output) = args.json_output.as_ref() {
        println!("- JSON Output: {}", json_output);
    }

    if args.test_mode {
        println!("- Test Mode: enabled");
    }

    println!("{DASH_LINE}");
    println!();
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
}
