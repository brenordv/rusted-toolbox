use crate::files::ResolvedRunContext;
use crate::models::{Cli, Command, DryRunArgs, ExecutionArgs, KeyValue, ListArgs, RunArgs};
use camino::Utf8PathBuf;
use clap::builder::ValueParser;
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;

pub fn get_cli_arguments() -> Cli {
    let matches = ClapCommand::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "This tool enables including one hurl file into another, and chaining their execution.",
        )
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(build_list_subcommand())
        .subcommand(build_run_subcommand())
        .subcommand(build_dry_run_subcommand())
        .get_matches();

    let command = match matches.subcommand() {
        Some(("list", sub_matches)) => Command::List(parse_list_args(sub_matches)),
        Some(("run", sub_matches)) => Command::Run(parse_run_args(sub_matches)),
        Some(("dry-run", sub_matches)) => Command::DryRun(parse_dry_run_args(sub_matches)),
        _ => unreachable!("clap enforces one of the known subcommands"),
    };

    Cli { command }
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

fn build_list_subcommand() -> ClapCommand {
    ClapCommand::new("list")
        .about("List available APIs or requests")
        .arg(
            Arg::new("api")
                .value_name("API")
                .help("API name to inspect. When omitted, prints all APIs.")
                .num_args(0..=1),
        )
}

fn build_run_subcommand() -> ClapCommand {
    add_execution_args(
        ClapCommand::new("run")
            .about("Expand includes and execute a request collection")
            .arg(
                Arg::new("json")
                    .long("json")
                    .value_name("PATH")
                    .value_parser(clap::value_parser!(Utf8PathBuf))
                    .help("Write execution JSON report to the given path."),
            )
            .arg(
                Arg::new("test")
                    .long("test")
                    .action(ArgAction::SetTrue)
                    .help("Emit a concise test-style summary after execution."),
            )
            .arg(
                Arg::new("print-only-full-response")
                    .long("print-only-full-response")
                    .action(ArgAction::SetTrue)
                    .help("Suppress header/log output and print only the final result object as JSON."),
            )
            .arg(
                Arg::new("print-only-response-body")
                    .long("print-only-response-body")
                    .action(ArgAction::SetTrue)
                    .help("Suppress header/log output and print only the last response body."),
            )
            .arg(
                Arg::new("silent")
                    .long("silent")
                    .action(ArgAction::SetTrue)
                    .help("Disable header/log output (behaves similar to legacy mode)."),
            ),
    )
}

fn build_dry_run_subcommand() -> ClapCommand {
    add_execution_args(
        ClapCommand::new("dry-run")
            .about("Expand includes and display the merged Hurl document")
            .arg(
                Arg::new("show-boundaries")
                    .long("show-boundaries")
                    .value_name("BOOL")
                    .num_args(0..=1)
                    .default_value("true")
                    .default_missing_value("true")
                    .value_parser(clap::value_parser!(bool))
                    .help("Print boundary markers between includes."),
            ),
    )
}

fn parse_list_args(matches: &ArgMatches) -> ListArgs {
    ListArgs {
        api: matches.get_one::<String>("api").cloned(),
    }
}

fn parse_run_args(matches: &ArgMatches) -> RunArgs {
    RunArgs {
        exec: parse_execution_args(matches),
        json_output: matches.get_one::<Utf8PathBuf>("json").cloned(),
        test_mode: matches.get_flag("test"),
        print_only_full_response: matches.get_flag("print-only-full-response"),
        print_only_response_body: matches.get_flag("print-only-response-body"),
        silent: matches.get_flag("silent"),
    }
}

fn parse_dry_run_args(matches: &ArgMatches) -> DryRunArgs {
    DryRunArgs {
        exec: parse_execution_args(matches),
        show_boundaries: matches
            .get_one::<bool>("show-boundaries")
            .copied()
            .unwrap_or(true),
    }
}

fn parse_execution_args(matches: &ArgMatches) -> ExecutionArgs {
    let api = matches
        .get_one::<String>("api")
        .cloned()
        .expect("`api` should be required by clap");
    let file = matches
        .get_one::<String>("file")
        .cloned()
        .expect("`file` should be required by clap");

    let inline_vars = matches
        .get_many::<KeyValue>("var")
        .map(|values| values.cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    ExecutionArgs {
        api,
        file,
        env: matches.get_one::<String>("env").cloned(),
        vars_file: matches.get_one::<Utf8PathBuf>("vars-file").cloned(),
        inline_vars,
        file_root: matches.get_one::<Utf8PathBuf>("file-root").cloned(),
        verbosity: matches.get_count("verbose") as u8,
    }
}

fn add_execution_args(command: ClapCommand) -> ClapCommand {
    command
        .arg(
            Arg::new("api")
                .value_name("API")
                .help("API directory containing the Hurl file.")
                .required(true),
        )
        .arg(
            Arg::new("file")
                .value_name("FILE")
                .help("Name of the Hurl file to execute (extension optional, relative to the API directory).")
                .required(true),
        )
        .arg(
            Arg::new("env")
                .long("env")
                .value_name("NAME")
                .help("Named environment to load from the API's vars directory."),
        )
        .arg(
            Arg::new("vars-file")
                .long("vars-file")
                .value_name("PATH")
                .value_parser(clap::value_parser!(Utf8PathBuf))
                .help("Path to an additional variables file (key=value pairs)."),
        )
        .arg(
            Arg::new("var")
                .long("var")
                .value_name("KEY=VALUE")
                .num_args(1)
                .action(ArgAction::Append)
                .value_parser(ValueParser::new(parse_key_value))
                .help("Provide an inline variable assignment (can be repeated)."),
        )
        .arg(
            Arg::new("file-root")
                .long("file-root")
                .value_name("PATH")
                .value_parser(clap::value_parser!(Utf8PathBuf))
                .help("Override the root directory for resolving file, responses, and captures."),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .action(ArgAction::Count)
                .help("Increase output verbosity. Pass twice for extra detail."),
        )
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
