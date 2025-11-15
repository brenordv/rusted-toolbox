use std::collections::{BTreeMap, BTreeSet};

use crate::cli_utils::print_runtime_info;
use crate::engine::run_hurl;
use crate::files::discover::{
    load_dynamic_vars_file, load_env_file, resolve_file_root, resolve_vars_file_path,
};
use crate::files::{
    list_apis, list_requests, locate_requests_root, FileResolver, ResolvedRunContext,
};
use crate::includer;
use crate::includer::Includer;
use crate::models::{
    Cli, Command, DryRunArgs, ListArgs, RunArgs, ToolError, ToolResult, VariableAccumulator,
};
use crate::output::{print_test_summary, write_json_report};
use crate::vars::{gather_process_env_variables, parse_variables_file, VariableMap};
use crate::whurl_utils::display_relative_path;
use anyhow::anyhow;
use camino::Utf8Path;
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

    let variables = build_variables(&resolver, &context, &include_result, &args, silent_mode)?;
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
    include_result: &includer::IncludeResult,
    args: &RunArgs,
    silent_mode: bool,
) -> ToolResult<VariableMap> {
    let mut merger = VariableAccumulator::new(!silent_mode);
    let env_vars = gather_process_env_variables();
    merger.extend_from_map(env_vars, "process environment (HURL_*)");

    let allow_shell = std::env::var("WHURL_ALLOW_DYN_SHELL_VARS")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or_else(|_| {
            std::env::var("WHURL_ALLOW_DYN_BASH_VARS")
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        });
    let log_dynamic = !silent_mode;

    let primary_api = context.resolution.api.clone();
    let mut additional_apis = BTreeSet::new();
    let requests_root = resolver.requests_root();

    for path in include_result.behaviors.keys() {
        if let Ok(relative) = path.strip_prefix(requests_root) {
            let mut components = relative.components();
            let Some(first) = components.next() else {
                continue;
            };
            let api_name = first.as_str();
            if api_name != primary_api {
                additional_apis.insert(api_name.to_string());
            }
        }
    }

    let mut included_vars_by_api: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current_file_vars: Vec<String> = Vec::new();

    for (path, directives) in &include_result.vars {
        if path == context.resolution.file_path.as_path() {
            for directive in directives {
                push_unique_case_insensitive(&mut current_file_vars, &directive.name);
            }
            continue;
        }

        if let Ok(relative) = path.strip_prefix(requests_root) {
            let mut components = relative.components();
            let Some(first) = components.next() else {
                continue;
            };
            let api_name = first.as_str().to_string();
            let entry = included_vars_by_api.entry(api_name).or_default();
            for directive in directives {
                push_unique_case_insensitive(entry, &directive.name);
            }
        }
    }

    let mut included_api_list: Vec<String> = additional_apis.into_iter().collect();
    included_api_list.sort();

    let mut loaded_dynamic: BTreeSet<(String, String)> = BTreeSet::new();

    // Included API calls (cross-API includes) hierarchy.
    for api in &included_api_list {
        if let Some((path, vars)) = load_env_file(resolver, &api, "_global", false)? {
            let origin = format!(
                "global vars file `{}`",
                display_relative_path(resolver, path.as_path())
            );
            merger.extend_from_map(vars, origin);
        }

        let _ = merge_dynamic_vars(
            &mut merger,
            resolver,
            &mut loaded_dynamic,
            &api,
            "_global",
            false,
            allow_shell,
            log_dynamic,
        )?;

        if let Some(env_name) = args.exec.env.as_ref() {
            let mut env_present = false;
            if let Some((path, vars)) = load_env_file(resolver, &api, env_name, false)? {
                let origin = format!(
                    "environment file `{}`",
                    display_relative_path(resolver, path.as_path())
                );
                merger.extend_from_map(vars, origin);
                env_present = true;
            }

            let dyn_present = merge_dynamic_vars(
                &mut merger,
                resolver,
                &mut loaded_dynamic,
                &api,
                env_name,
                false,
                allow_shell,
                log_dynamic,
            )?;

            if dyn_present {
                env_present = true;
            }

            if !env_present {
                return Err(ToolError::Other(anyhow!(
                    "environment `{}` not found for api `{}`",
                    env_name,
                    api
                )));
            }
        }
    }

    // Imported dynamic vars from included files (any API).
    let mut included_dyn_entries: Vec<_> = included_vars_by_api.into_iter().collect();
    included_dyn_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (api, names) in included_dyn_entries {
        for name in names {
            let _ = merge_dynamic_vars(
                &mut merger,
                resolver,
                &mut loaded_dynamic,
                &api,
                &name,
                true,
                allow_shell,
                log_dynamic,
            )?;
        }
    }

    // Current API hierarchy.
    if let Some((path, vars)) = load_env_file(resolver, &primary_api, "_global", false)? {
        let origin = format!(
            "global vars file `{}`",
            display_relative_path(resolver, path.as_path())
        );
        merger.extend_from_map(vars, origin);
    }

    let _ = merge_dynamic_vars(
        &mut merger,
        resolver,
        &mut loaded_dynamic,
        &primary_api,
        "_global",
        false,
        allow_shell,
        log_dynamic,
    )?;

    if let Some(env_name) = args.exec.env.as_ref() {
        let mut env_present = false;
        if let Some((path, vars)) = load_env_file(resolver, &primary_api, env_name, false)? {
            let origin = format!(
                "environment file `{}`",
                display_relative_path(resolver, path.as_path())
            );
            merger.extend_from_map(vars, origin);
            env_present = true;
        }

        let dyn_present = merge_dynamic_vars(
            &mut merger,
            resolver,
            &mut loaded_dynamic,
            &primary_api,
            env_name,
            true,
            allow_shell,
            log_dynamic,
        )?;

        if dyn_present {
            env_present = true;
        }

        if !env_present {
            return Err(ToolError::Other(anyhow!(
                "environment `{}` not found for api `{}`",
                env_name,
                primary_api
            )));
        }
    }

    for name in current_file_vars {
        let _ = merge_dynamic_vars(
            &mut merger,
            resolver,
            &mut loaded_dynamic,
            &primary_api,
            &name,
            true,
            allow_shell,
            log_dynamic,
        )?;
    }

    if let Some(vars_file) = args.exec.vars_file.as_ref() {
        let resolved = resolve_vars_file_path(&context.resolution.api_root, vars_file);
        let parsed = parse_variables_file(resolved.as_path())?;
        let origin = format!(
            "vars file `{}`",
            display_relative_path(resolver, resolved.as_path())
        );
        merger.extend_from_map(parsed, origin);
    }

    for kv in &args.exec.inline_vars {
        merger.insert(
            kv.key.clone(),
            kv.value.clone(),
            format!("inline argument `{}`", kv),
        );
    }

    Ok(merger.finish())
}

fn merge_dynamic_vars(
    merger: &mut VariableAccumulator,
    resolver: &FileResolver,
    loaded_dynamic: &mut BTreeSet<(String, String)>,
    api: &str,
    name: &str,
    required: bool,
    allow_shell: bool,
    log_dynamic: bool,
) -> ToolResult<bool> {
    let key = (api.to_ascii_lowercase(), name.to_ascii_lowercase());
    if loaded_dynamic.contains(&key) {
        return Ok(true);
    }

    if let Some((path, vars)) =
        load_dynamic_vars_file(resolver, api, name, required, allow_shell, log_dynamic)?
    {
        let origin = format!(
            "dynamic vars file `{}`",
            display_relative_path(resolver, path.as_path())
        );
        merger.extend_from_map(vars, origin);
        loaded_dynamic.insert(key);
        return Ok(true);
    }

    Ok(false)
}

fn push_unique_case_insensitive(vec: &mut Vec<String>, value: &str) {
    if value.is_empty() {
        return;
    }

    if !vec
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        vec.push(value.to_string());
    }
}
