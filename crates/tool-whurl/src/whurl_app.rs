use crate::cli_utils::print_runtime_info;
use crate::engine::run_hurl;
use crate::files::{
    list_apis, list_requests, locate_requests_root, FileResolver, ResolvedRunContext,
};
use crate::includer;
use crate::includer::Includer;
use crate::models::{Cli, Command, DryRunArgs, ListArgs, RunArgs, ToolError, ToolResult};
use crate::output::{print_test_summary, write_json_report};
use crate::vars::{
    gather_process_env_variables, merge_variable_sources, parse_variables_file, VariableMap,
};
use camino::{Utf8Path, Utf8PathBuf};
use shared::logging::app_logger::LogLevel;
use tracing::info;

pub fn execute(cli: Cli) -> ToolResult<()> {
    match cli.command {
        Command::List(args) => handle_list(args),
        Command::DryRun(args) => handle_dry_run(args),
        Command::Run(args) => handle_run(args),
    }
}

pub fn resolve_log_level(cli: &Cli) -> LogLevel {
    match &cli.command {
        Command::Run(args) if args.print_only_result || args.silent => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

pub fn print_error(error: &ToolError) {
    eprintln!("Error: {error}");
    match error {
        ToolError::Include(inner) => eprintln!("Caused by includer: {inner}"),
        ToolError::Resolve(inner) => eprintln!("Path resolution failed: {inner}"),
        ToolError::Discover(inner) => eprintln!("Discovery failed: {inner}"),
        ToolError::Vars(inner) => eprintln!("Variable error: {inner}"),
        ToolError::Output(inner) => eprintln!("Output error: {inner}"),
        ToolError::Engine(inner) => eprintln!("Engine error: {inner}"),
        ToolError::Other(inner) => eprintln!("{inner}"),
        ToolError::ExecutionFailure => eprintln!("One or more requests failed."),
    }
}

fn handle_list(args: ListArgs) -> ToolResult<()> {
    let requests_root = locate_requests_root()?;

    match args.api {
        Some(api) => {
            let requests = list_requests(&requests_root, &api)?;
            if requests.is_empty() {
                println!("(no requests found under `{api}`)");
            } else {
                for request in requests {
                    println!("{request}");
                }
            }
        }
        None => {
            let apis = list_apis(&requests_root)?;
            if apis.is_empty() {
                println!("(no APIs discovered in `{}`)", requests_root);
            } else {
                for api in apis {
                    println!("{api}");
                }
            }
        }
    }

    Ok(())
}

fn handle_dry_run(args: DryRunArgs) -> ToolResult<()> {
    let requests_root = locate_requests_root()?;
    let resolver = FileResolver::new(requests_root.clone());

    let context = resolver.resolve_run_context(&args.exec.api, &args.exec.file)?;
    let mut includer = Includer::new(resolver.clone());
    includer = includer.with_boundaries(args.show_boundaries);
    let result = includer.merge(context.resolution.file_path.as_path())?;

    println!("{}", result.merged);
    Ok(())
}

fn handle_run(args: RunArgs) -> ToolResult<()> {
    let requests_root = locate_requests_root()?;
    let resolver = FileResolver::new(requests_root.clone());

    let silent_mode = args.silent || args.print_only_result;

    let context = resolver.resolve_run_context(&args.exec.api, &args.exec.file)?;
    let include_result =
        Includer::new(resolver.clone()).merge(context.resolution.file_path.as_path())?;

    if !silent_mode {
        print_runtime_info(&context, &args);
    }

    let variables = build_variables(&resolver, &context, &args)?;
    let file_root = resolve_file_root(&context, args.exec.file_root.as_ref());

    let result = run_hurl(
        include_result.merged.as_str(),
        &context.display_path,
        &variables,
        args.exec.verbosity,
        file_root.as_deref(),
    )?;

    if let Some(json_path) = args.json_output.as_ref() {
        write_json_report(
            &result,
            include_result.merged.as_str(),
            &context.display_path,
            json_path.as_path(),
        )?;
    }

    if args.print_only_result {
        let stdout_path = Utf8Path::new("-");
        write_json_report(
            &result,
            include_result.merged.as_str(),
            &context.display_path,
            stdout_path,
        )?;
    } else {
        if !silent_mode {
            log_execution_details(&result, &include_result);
        }

        if args.test_mode {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            print_test_summary(
                &mut handle,
                &result,
                &include_result,
                resolver.requests_root(),
            )?;
        }
    }

    if !result.success {
        return Err(ToolError::ExecutionFailure);
    }

    Ok(())
}

fn log_execution_details(result: &hurl::runner::HurlResult, includes: &includer::IncludeResult) {
    for entry in &result.entries {
        let entry_behavior = includes
            .map_source(&entry.source_info)
            .map(|mapping| includes.behavior_for(mapping.source.as_path()))
            .unwrap_or_default();

        if entry_behavior.silent {
            continue;
        }

        if entry.calls.is_empty() {
            info!("Entry #{} executed with no HTTP calls.", entry.entry_index);
            continue;
        }

        for (idx, call) in entry.calls.iter().enumerate() {
            info!(
                "Entry #{} Call #{} → {} {}",
                entry.entry_index,
                idx + 1,
                call.request.method,
                call.request.url
            );

            info!(
                "Status: {} ({:?})",
                call.response.status, call.response.version
            );

            if !entry_behavior.quiet {
                if let Some(formatted_body) = format_response_body(call) {
                    info!("Response Body:\n{}", formatted_body);
                }
            }
        }
    }
}

fn format_response_body(call: &hurl::http::Call) -> Option<String> {
    if call.response.body.is_empty() {
        return None;
    }

    let is_json = call
        .response
        .headers
        .get("content-type")
        .map(|header| header.value.to_ascii_lowercase().contains("json"))
        .unwrap_or(false);

    if is_json {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&call.response.body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                return Some(pretty);
            }
        }
    }

    match String::from_utf8(call.response.body.clone()) {
        Ok(text) => Some(text),
        Err(_) => Some(format!(
            "<{} bytes of non-UTF8 data>",
            call.response.body.len()
        )),
    }
}

fn build_variables(
    resolver: &FileResolver,
    context: &ResolvedRunContext,
    args: &RunArgs,
) -> ToolResult<VariableMap> {
    let mut file_vars = VariableMap::new();

    if let Some(env_name) = args.exec.env.as_ref() {
        let env_file = resolver.resolve_env_file(&context.resolution.api, env_name)?;
        let parsed = parse_variables_file(env_file.as_path())?;
        file_vars.extend(parsed);
    }

    if let Some(vars_file) = args.exec.vars_file.as_ref() {
        let resolved = resolve_vars_file_path(&context.resolution.api_root, vars_file);
        let parsed = parse_variables_file(resolved.as_path())?;
        for (key, value) in parsed {
            file_vars.insert(key, value);
        }
    }

    let inline_vars = args
        .exec
        .inline_vars
        .iter()
        .map(|kv| (kv.key.clone(), kv.value.clone()))
        .collect::<Vec<_>>();

    let env_vars = gather_process_env_variables();
    let file_vars_option = if file_vars.is_empty() {
        None
    } else {
        Some(file_vars)
    };

    Ok(merge_variable_sources(
        env_vars,
        file_vars_option,
        &inline_vars,
    ))
}

fn resolve_vars_file_path(api_root: &Utf8Path, vars_file: &Utf8PathBuf) -> Utf8PathBuf {
    if vars_file.is_absolute() {
        vars_file.clone()
    } else if vars_file.as_path().is_file() {
        vars_file.clone()
    } else {
        let candidate = api_root.join(vars_file);
        if candidate.as_path().is_file() {
            candidate
        } else {
            vars_file.clone()
        }
    }
}

fn resolve_file_root(
    context: &ResolvedRunContext,
    file_root: Option<&Utf8PathBuf>,
) -> Option<Utf8PathBuf> {
    let file_root = file_root?;

    if file_root.is_absolute() {
        Some(file_root.clone())
    } else {
        Some(context.resolution.api_root.join(file_root))
    }
}
