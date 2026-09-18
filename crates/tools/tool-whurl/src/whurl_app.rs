use std::collections::{BTreeMap, BTreeSet};

use crate::engine::run_hurl;
use crate::files::discover::{
    load_dynamic_vars_file, load_env_file, resolve_file_root, resolve_vars_file_path,
};
use crate::files::{
    FileResolver, ResolvedRunContext, list_apis, list_requests, locate_requests_root,
};
use crate::includer;
use crate::includer::Includer;
use crate::includer::{FeedValue, IncludeFeed};
use crate::models::{
    Cli, Command, DryRunArgs, KeyValue, ListArgs, RunArgs, ToolError, ToolResult,
    VariableAccumulator,
};
use crate::output::{print_test_summary, render_json_report, write_json_report};
use crate::vars::{
    DynamicEvalContext, VariableMap, gather_process_env_variables, parse_variables_file,
};
use crate::whurl_utils::{ElapsedTracker, display_relative_path, format_elapsed_line};
use anyhow::anyhow;
use camino::Utf8Path;
use tracing::{info, warn};

pub fn execute(cli: Cli) -> ToolResult<()> {
    match cli.command {
        Command::List(args) => handle_list(args),
        Command::DryRun(args) => handle_dry_run(args),
        Command::Run(args) => handle_run(args),
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

    let silent_mode = args.silent_mode();

    let context = resolver.resolve_run_context(&args.exec.api, &args.exec.file)?;
    let include_result =
        Includer::new(resolver.clone()).merge(context.resolution.file_path.as_path())?;

    let api_env_overrides = reduce_env_overrides(&resolver, &include_result);
    let variables = build_variables(
        &resolver,
        &context,
        &include_result,
        &api_env_overrides,
        &args,
        silent_mode,
    )?;
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

    if args.print_only_full_response {
        print_full_response_pretty(
            &result,
            include_result.merged.as_str(),
            &context.display_path,
        )?;
    } else if args.print_only_response_body {
        print_only_response_body(&result);
    } else {
        if !silent_mode {
            log_execution_details(
                &result,
                &include_result,
                resolver.requests_root(),
                &api_env_overrides,
                args.exec.env.as_deref(),
            );
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

fn log_execution_details(
    result: &hurl::runner::HurlResult,
    includes: &includer::IncludeResult,
    requests_root: &Utf8Path,
    env_overrides: &BTreeMap<String, includer::EnvOverride>,
    default_env: Option<&str>,
) {
    let mut elapsed_tracker = ElapsedTracker::default();

    for entry in &result.entries {
        let mapping = includes.map_source(&entry.source_info);

        // The entry's effective environment: its source API's override when
        // one exists, the run's --env otherwise.
        let entry_env = mapping
            .and_then(|mapping| api_of(requests_root, mapping.source.as_path()))
            .and_then(|api| env_overrides.get(&api))
            .map(|env_override| env_override.name.clone())
            .or_else(|| default_env.map(str::to_string));

        // The history key is the entry's first call (the request as written);
        // redirects keep the entry under the same key.
        let history_key = entry.calls.first().map(|call| {
            format!(
                "{} {}|{}",
                call.request.method,
                call.request.url,
                entry_env.as_deref().unwrap_or("-")
            )
        });
        // Observed before any display suppression, so the running total covers
        // every entry, silent includes included.
        let elapsed = elapsed_tracker.observe(history_key, entry.transfer_duration);

        let entry_behavior = mapping
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

        info!("{}", format_elapsed_line(&elapsed));
    }
}

fn print_only_response_body(result: &hurl::runner::HurlResult) {
    let last_call = result
        .entries
        .iter()
        .flat_map(|entry| entry.calls.iter())
        .last();

    let Some(call) = last_call else {
        println!();
        return;
    };

    let status_is_204 = call.response.status.to_string().trim() == "204";
    if status_is_204 {
        println!();
        return;
    }

    let Some(formatted_body) = format_response_body(call) else {
        println!();
        return;
    };

    if formatted_body.is_empty() {
        println!();
        return;
    }

    print!("{}", formatted_body);
    if !formatted_body.ends_with('\n') {
        println!();
    }
}

fn print_full_response_pretty(
    result: &hurl::runner::HurlResult,
    merged: &str,
    display_path: &str,
) -> ToolResult<()> {
    let contents = render_json_report(result, merged, display_path)?;

    if contents.trim().is_empty() {
        println!();
        return Ok(());
    }

    match serde_json::from_str::<serde_json::Value>(&contents) {
        Ok(value) => {
            let pretty = serde_json::to_string_pretty(&value).map_err(|source| {
                ToolError::Other(anyhow!("failed to pretty print JSON report: {source}"))
            })?;
            println!("{pretty}");
        }
        Err(_) => {
            println!("{contents}");
        }
    }

    Ok(())
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

/// The API an absolute path under the requests root belongs to (its first
/// path component), when it has one.
fn api_of(requests_root: &Utf8Path, path: &Utf8Path) -> Option<String> {
    let relative = path.strip_prefix(requests_root).ok()?;
    let first = relative.components().next()?;
    Some(first.as_str().to_string())
}

/// Reduces the per-include-path environment overrides to one effective
/// override per API: env layers load per API, so the first override
/// registered for an API wins and later differing ones warn.
fn reduce_env_overrides(
    resolver: &FileResolver,
    include_result: &includer::IncludeResult,
) -> BTreeMap<String, includer::EnvOverride> {
    let mut by_api: BTreeMap<String, includer::EnvOverride> = BTreeMap::new();

    for (path, env_override) in &include_result.env_overrides {
        let Some(api) = api_of(resolver.requests_root(), path.as_path()) else {
            continue;
        };

        match by_api.get(&api) {
            Some(existing) if existing.name != env_override.name => {
                warn!(
                    api = %api,
                    kept = %existing.name,
                    ignored = %env_override.name,
                    "conflicting include environment overrides for one API; the first wins"
                );
            }
            Some(_) => {}
            None => {
                by_api.insert(api, env_override.clone());
            }
        }
    }

    by_api
}

fn build_variables(
    resolver: &FileResolver,
    context: &ResolvedRunContext,
    include_result: &includer::IncludeResult,
    env_overrides: &BTreeMap<String, includer::EnvOverride>,
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
    // One context for the whole pass, so a $shell expression duplicated across
    // layers or APIs executes once per run.
    let mut dyn_ctx = DynamicEvalContext::new(allow_shell, !silent_mode);

    let primary_api = context.resolution.api.clone();
    let mut additional_apis = BTreeSet::new();
    let requests_root = resolver.requests_root();

    for path in include_result.behaviors.keys() {
        let Some(api_name) = api_of(requests_root, path.as_path()) else {
            continue;
        };
        if api_name != primary_api {
            additional_apis.insert(api_name);
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

        if let Some(api_name) = api_of(requests_root, path.as_path()) {
            let entry = included_vars_by_api.entry(api_name).or_default();
            for directive in directives {
                push_unique_case_insensitive(entry, &directive.name);
            }
        }
    }

    let mut included_api_list: Vec<String> = additional_apis.into_iter().collect();
    included_api_list.sort();

    let mut loaded_dynamic: BTreeSet<(String, String)> = BTreeSet::new();
    let mut loaded_static_directives: BTreeSet<(String, String)> = BTreeSet::new();

    // Included API calls (cross-API includes) hierarchy.
    for api in &included_api_list {
        if let Some((path, vars)) = load_env_file(resolver, api, "_global", false)? {
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
            api,
            "_global",
            false,
            &mut dyn_ctx,
        )?;

        merge_effective_env_layers(
            &mut merger,
            resolver,
            &mut loaded_dynamic,
            api,
            env_overrides,
            args.exec.env.as_deref(),
            &mut dyn_ctx,
        )?;
    }

    // Imported dynamic vars from included files (any API).
    let mut included_dyn_entries: Vec<_> = included_vars_by_api.into_iter().collect();
    included_dyn_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (api, names) in included_dyn_entries {
        for name in names {
            merge_directive_vars(
                &mut merger,
                resolver,
                &mut loaded_static_directives,
                &mut loaded_dynamic,
                &api,
                &name,
                &mut dyn_ctx,
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
        &mut dyn_ctx,
    )?;

    merge_effective_env_layers(
        &mut merger,
        resolver,
        &mut loaded_dynamic,
        &primary_api,
        env_overrides,
        args.exec.env.as_deref(),
        &mut dyn_ctx,
    )?;

    for name in current_file_vars {
        merge_directive_vars(
            &mut merger,
            resolver,
            &mut loaded_static_directives,
            &mut loaded_dynamic,
            &primary_api,
            &name,
            &mut dyn_ctx,
        )?;
    }

    // Include feeds apply after every file-based layer and before the CLI
    // layers, so a feed beats the files and loses only to --vars-file/--var.
    // References resolve to the variable's final value for CLI-supplied names
    // (those layers apply later but their values are known up front); the
    // vars file is parsed early only when a reference exists, so feed-free
    // runs keep their error ordering.
    let hoisted_vars_file = if feeds_have_references(&include_result.feeds) {
        match args.exec.vars_file.as_ref() {
            Some(vars_file) => {
                let resolved = resolve_vars_file_path(&context.resolution.api_root, vars_file);
                Some(parse_variables_file(resolved.as_path())?)
            }
            None => None,
        }
    } else {
        None
    };

    for feed in &include_result.feeds {
        let origin = format!(
            "include feed for `{}` at `{}:{}`",
            display_relative_path(resolver, feed.include_path.as_path()),
            display_relative_path(resolver, feed.from_file.as_path()),
            feed.line
        );

        for assignment in &feed.assignments {
            let value = match &assignment.value {
                FeedValue::Literal(value) => value.clone(),
                FeedValue::Reference(name) => resolve_feed_reference(
                    name,
                    &args.exec.inline_vars,
                    hoisted_vars_file.as_ref(),
                    &merger,
                )
                .ok_or_else(|| {
                    ToolError::Other(anyhow!(
                        "include feed at `{}:{}` references unknown variable `{}`",
                        display_relative_path(resolver, feed.from_file.as_path()),
                        feed.line,
                        name
                    ))
                })?,
            };

            merger.insert(assignment.key.clone(), value, origin.clone());
        }
    }

    if let Some(vars_file) = args.exec.vars_file.as_ref() {
        let resolved = resolve_vars_file_path(&context.resolution.api_root, vars_file);
        let parsed = match hoisted_vars_file {
            Some(parsed) => parsed,
            None => parse_variables_file(resolved.as_path())?,
        };
        let origin = format!(
            "vars file `{}`",
            display_relative_path(resolver, resolved.as_path())
        );
        merger.extend_from_map(parsed, origin);
    }

    for kv in &args.exec.inline_vars {
        // The origin names the key only; origins reach collision warnings and
        // the tracing stream can end up in a log file.
        merger.insert(
            kv.key.clone(),
            kv.value.clone(),
            format!("inline argument `--var {}`", kv.key),
        );
    }

    Ok(merger.finish())
}

fn feeds_have_references(feeds: &[IncludeFeed]) -> bool {
    feeds.iter().any(|feed| {
        feed.assignments
            .iter()
            .any(|assignment| matches!(assignment.value, FeedValue::Reference(_)))
    })
}

/// Resolves a feed `{{name}}` against the variable's eventual final value:
/// `--var` first (last occurrence wins, matching apply order), the
/// `--vars-file` map next, then everything merged so far (file layers and
/// earlier feeds).
fn resolve_feed_reference(
    name: &str,
    inline_vars: &[KeyValue],
    vars_file: Option<&VariableMap>,
    merger: &VariableAccumulator,
) -> Option<String> {
    if let Some(kv) = inline_vars.iter().rev().find(|kv| kv.key == name) {
        return Some(kv.value.clone());
    }

    if let Some(value) = vars_file.and_then(|map| map.get(name)) {
        return Some(value.clone());
    }

    merger.get(name).map(str::to_string)
}

fn merge_dynamic_vars(
    merger: &mut VariableAccumulator,
    resolver: &FileResolver,
    loaded_dynamic: &mut BTreeSet<(String, String)>,
    api: &str,
    name: &str,
    required: bool,
    ctx: &mut DynamicEvalContext,
) -> ToolResult<bool> {
    let key = (api.to_ascii_lowercase(), name.to_ascii_lowercase());
    if loaded_dynamic.contains(&key) {
        return Ok(true);
    }

    if let Some((path, vars)) = load_dynamic_vars_file(resolver, api, name, required, ctx)? {
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

/// Applies the env layers for one API using its effective environment: the
/// include override when one exists (it applies even when `--env` was not
/// passed), the run's `--env` otherwise, nothing when neither is set.
fn merge_effective_env_layers(
    merger: &mut VariableAccumulator,
    resolver: &FileResolver,
    loaded_dynamic: &mut BTreeSet<(String, String)>,
    api: &str,
    env_overrides: &BTreeMap<String, includer::EnvOverride>,
    default_env: Option<&str>,
    ctx: &mut DynamicEvalContext,
) -> ToolResult<()> {
    let override_source = env_overrides.get(api);
    let env_name = match override_source {
        Some(env_override) => {
            info!(
                api = %api,
                env = %env_override.name,
                replaces = default_env.unwrap_or("<none>"),
                directive = %format!(
                    "{}:{}",
                    display_relative_path(resolver, env_override.from_file.as_path()),
                    env_override.line
                ),
                "include environment override applied"
            );
            env_override.name.as_str()
        }
        None => match default_env {
            Some(env_name) => env_name,
            None => return Ok(()),
        },
    };

    merge_env_layers(
        merger,
        resolver,
        loaded_dynamic,
        api,
        env_name,
        override_source,
        ctx,
    )
}

fn merge_env_layers(
    merger: &mut VariableAccumulator,
    resolver: &FileResolver,
    loaded_dynamic: &mut BTreeSet<(String, String)>,
    api: &str,
    env_name: &str,
    override_source: Option<&includer::EnvOverride>,
    ctx: &mut DynamicEvalContext,
) -> ToolResult<()> {
    let mut env_present = false;
    if let Some((path, vars)) = load_env_file(resolver, api, env_name, false)? {
        let origin = format!(
            "environment file `{}`",
            display_relative_path(resolver, path.as_path())
        );
        merger.extend_from_map(vars, origin);
        env_present = true;
    }

    let dyn_present =
        merge_dynamic_vars(merger, resolver, loaded_dynamic, api, env_name, false, ctx)?;

    if dyn_present {
        env_present = true;
    }

    if !env_present {
        let citation = override_source
            .map(|env_override| {
                format!(
                    " (set via `# @include:[env=...]` at {}:{})",
                    display_relative_path(resolver, env_override.from_file.as_path()),
                    env_override.line
                )
            })
            .unwrap_or_default();
        return Err(ToolError::Other(anyhow!(
            "environment `{}` not found for api `{}`{}",
            env_name,
            api,
            citation
        )));
    }

    Ok(())
}

fn merge_directive_vars(
    merger: &mut VariableAccumulator,
    resolver: &FileResolver,
    loaded_static: &mut BTreeSet<(String, String)>,
    loaded_dynamic: &mut BTreeSet<(String, String)>,
    api: &str,
    name: &str,
    ctx: &mut DynamicEvalContext,
) -> ToolResult<()> {
    let static_present = merge_static_vars(merger, resolver, loaded_static, api, name)?;
    let dynamic_present =
        merge_dynamic_vars(merger, resolver, loaded_dynamic, api, name, false, ctx)?;

    if !static_present && !dynamic_present {
        return Err(ToolError::Other(anyhow!(
            "vars directive `{}` not found for api `{}`; expected either `{name}.hurlvars` or `{name}.dvars`",
            name,
            api
        )));
    }

    Ok(())
}

fn merge_static_vars(
    merger: &mut VariableAccumulator,
    resolver: &FileResolver,
    loaded_static: &mut BTreeSet<(String, String)>,
    api: &str,
    name: &str,
) -> ToolResult<bool> {
    let key = (api.to_ascii_lowercase(), name.to_ascii_lowercase());
    if loaded_static.contains(&key) {
        return Ok(true);
    }

    if let Some((path, vars)) = load_env_file(resolver, api, name, false)? {
        let origin = format!(
            "# @vars `{}` hurlvars `{}`",
            name,
            display_relative_path(resolver, path.as_path())
        );
        merger.extend_from_map(vars, origin);
        loaded_static.insert(key);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ExecutionArgs, KeyValue};
    use camino::Utf8PathBuf;
    use std::fs;
    use tempfile::tempdir;

    fn create_resolver() -> (tempfile::TempDir, FileResolver) {
        let temp = tempdir().expect("tempdir");
        let root_path = temp.path().join("requests");
        fs::create_dir_all(root_path.join("api/_vars")).expect("requests dirs");
        let root_utf8 = Utf8PathBuf::from_path_buf(root_path).expect("utf8 path");
        let resolver = FileResolver::new(root_utf8);
        (temp, resolver)
    }

    fn run_args(api: &str, file: &str, env: Option<&str>) -> RunArgs {
        RunArgs {
            exec: ExecutionArgs {
                api: api.to_string(),
                file: file.to_string(),
                env: env.map(str::to_string),
                vars_file: None,
                inline_vars: Vec::new(),
                file_root: None,
                verbosity: 0,
            },
            json_output: None,
            test_mode: false,
            print_only_full_response: false,
            print_only_response_body: false,
            silent: true,
        }
    }

    fn build_vars_with(resolver: &FileResolver, args: RunArgs) -> ToolResult<VariableMap> {
        let context = resolver
            .resolve_run_context(&args.exec.api, &args.exec.file)
            .expect("run context");
        let include_result = Includer::new(resolver.clone())
            .merge(context.resolution.file_path.as_path())
            .expect("merge");
        let env_overrides = reduce_env_overrides(resolver, &include_result);
        build_variables(
            resolver,
            &context,
            &include_result,
            &env_overrides,
            &args,
            true,
        )
    }

    fn build_vars_for(
        resolver: &FileResolver,
        api: &str,
        file: &str,
        env: Option<&str>,
    ) -> ToolResult<VariableMap> {
        build_vars_with(resolver, run_args(api, file, env))
    }

    #[test]
    fn vars_directive_loads_hurlvars_before_dvars() {
        let (temp, resolver) = create_resolver();
        let vars_dir = temp.path().join("requests/api/_vars");
        fs::write(vars_dir.join("session.hurlvars"), "TOKEN=static\n").unwrap();
        fs::write(
            vars_dir.join("session.dvars"),
            r#"TOKEN=$random["dynamic"]"#,
        )
        .unwrap();

        let mut merger = VariableAccumulator::new(false);
        let mut loaded_static = BTreeSet::new();
        let mut loaded_dynamic = BTreeSet::new();
        let mut ctx = DynamicEvalContext::default();

        merge_directive_vars(
            &mut merger,
            &resolver,
            &mut loaded_static,
            &mut loaded_dynamic,
            "api",
            "session",
            &mut ctx,
        )
        .expect("merge directive vars");

        let vars = merger.finish();
        assert_eq!(vars.get("TOKEN").map(|s| s.as_str()), Some("dynamic"));
    }

    #[test]
    fn vars_directive_requires_at_least_one_file() {
        let (_temp, resolver) = create_resolver();
        let mut merger = VariableAccumulator::new(false);
        let mut loaded_static = BTreeSet::new();
        let mut loaded_dynamic = BTreeSet::new();
        let mut ctx = DynamicEvalContext::default();

        let err = merge_directive_vars(
            &mut merger,
            &resolver,
            &mut loaded_static,
            &mut loaded_dynamic,
            "api",
            "missing",
            &mut ctx,
        )
        .expect_err("missing vars should fail");

        assert!(matches!(err, ToolError::Other(_)));
        assert!(
            err.to_string()
                .contains("vars directive `missing` not found for api `api`")
        );
    }

    #[test]
    fn include_feed_beats_file_layers_and_loses_to_cli() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other/_vars")).expect("other dirs");
        fs::write(
            temp.path().join("requests/other/_vars/_global.hurlvars"),
            "fed=global\ncli=global\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include other/req -> { fed=feed, cli=feed }\nGET https://example.com\n",
        )
        .unwrap();

        let mut args = run_args("api", "request", None);
        args.exec.inline_vars = vec![KeyValue {
            key: "cli".to_string(),
            value: "cli".to_string(),
        }];

        let vars = build_vars_with(&resolver, args).expect("build variables");
        assert_eq!(vars.get("fed").map(String::as_str), Some("feed"));
        assert_eq!(vars.get("cli").map(String::as_str), Some("cli"));
    }

    #[test]
    fn later_feed_wins_when_two_feeds_set_the_same_key() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other")).expect("other dir");
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/other/req2.hurl"),
            "GET https://example.com/other2\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include other/req -> { shared=first }\n# @include other/req2 -> { shared=second }\nGET https://example.com\n",
        )
        .unwrap();

        let vars = build_vars_for(&resolver, "api", "request", None).expect("build variables");
        assert_eq!(vars.get("shared").map(String::as_str), Some("second"));
    }

    #[test]
    fn feed_references_resolve_from_var_vars_file_and_file_layers() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other")).expect("other dir");
        fs::write(
            temp.path().join("requests/api/_vars/_global.hurlvars"),
            "from_global=v3\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/extra.vars"),
            "from_file=v2\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include other/req -> { a={{from_var}}, b={{from_file}}, c={{from_global}} }\nGET https://example.com\n",
        )
        .unwrap();

        let mut args = run_args("api", "request", None);
        args.exec.vars_file = Some(Utf8PathBuf::from("extra.vars"));
        args.exec.inline_vars = vec![KeyValue {
            key: "from_var".to_string(),
            value: "v1".to_string(),
        }];

        let vars = build_vars_with(&resolver, args).expect("build variables");
        assert_eq!(vars.get("a").map(String::as_str), Some("v1"));
        assert_eq!(vars.get("b").map(String::as_str), Some("v2"));
        assert_eq!(vars.get("c").map(String::as_str), Some("v3"));
    }

    #[test]
    fn unknown_feed_reference_errors_with_the_directive_location() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other")).expect("other dir");
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include other/req -> { x={{ghost}} }\nGET https://example.com\n",
        )
        .unwrap();

        let err = build_vars_for(&resolver, "api", "request", None)
            .expect_err("unknown reference must fail");
        let message = err.to_string();
        let cited = format!("api{}request.hurl", std::path::MAIN_SEPARATOR);
        assert!(
            message.contains(&format!(
                "include feed at `{cited}:1` references unknown variable `ghost`"
            )),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn include_env_override_loads_env_layers_without_cli_env() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other/_vars")).expect("other dirs");
        fs::write(
            temp.path().join("requests/other/_vars/dev.hurlvars"),
            "from_dev=yes\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include:[env=dev] other/req\nGET https://example.com\n",
        )
        .unwrap();

        let vars = build_vars_for(&resolver, "api", "request", None).expect("build variables");
        assert_eq!(vars.get("from_dev").map(String::as_str), Some("yes"));
    }

    #[test]
    fn include_env_override_beats_the_cli_env_for_that_api_only() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other/_vars")).expect("other dirs");
        fs::write(
            temp.path().join("requests/other/_vars/dev.hurlvars"),
            "other_env=dev\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/other/_vars/local.hurlvars"),
            "other_env=local\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/_vars/local.hurlvars"),
            "primary_env=local\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include:[env=dev] other/req\nGET https://example.com\n",
        )
        .unwrap();

        let vars =
            build_vars_for(&resolver, "api", "request", Some("local")).expect("build variables");
        assert_eq!(vars.get("other_env").map(String::as_str), Some("dev"));
        assert_eq!(vars.get("primary_env").map(String::as_str), Some("local"));
    }

    #[test]
    fn missing_override_env_error_cites_the_directive() {
        let (temp, resolver) = create_resolver();
        fs::create_dir_all(temp.path().join("requests/other")).expect("other dir");
        fs::write(
            temp.path().join("requests/other/req.hurl"),
            "GET https://example.com/other\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include:[env=ghost] other/req\nGET https://example.com\n",
        )
        .unwrap();

        let err = build_vars_for(&resolver, "api", "request", None)
            .expect_err("missing override env must fail");
        let message = err.to_string();
        assert!(message.contains("environment `ghost` not found for api `other`"));
        let cited = format!("api{}request.hurl", std::path::MAIN_SEPARATOR);
        assert!(message.contains(&format!("set via `# @include:[env=...]` at {cited}:1")));
    }

    #[test]
    fn primary_api_include_override_redirects_the_primary_env_layer() {
        let (temp, resolver) = create_resolver();
        fs::write(
            temp.path().join("requests/api/_vars/dev.hurlvars"),
            "from_dev=yes\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/helper.hurl"),
            "GET https://example.com/helper\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("requests/api/request.hurl"),
            "# @include:[env=dev] helper\nGET https://example.com\n",
        )
        .unwrap();

        let vars = build_vars_for(&resolver, "api", "request", None).expect("build variables");
        assert_eq!(vars.get("from_dev").map(String::as_str), Some("yes"));
    }

    #[test]
    fn build_variables_layers_inline_over_vars_file_over_file_vars_over_global() {
        let (temp, resolver) = create_resolver();
        let api_root =
            Utf8PathBuf::from_path_buf(temp.path().join("requests/api")).expect("utf8 api root");
        let vars_dir = api_root.join("_vars");

        fs::write(
            vars_dir.join("_global.hurlvars").as_std_path(),
            "layer_global=global\nlayer_file=global\nlayer_varsfile=global\nlayer_inline=global\n",
        )
        .unwrap();
        fs::write(
            vars_dir.join("session.hurlvars").as_std_path(),
            "layer_file=session\nlayer_varsfile=session\nlayer_inline=session\n",
        )
        .unwrap();
        fs::write(
            api_root.join("extra.vars").as_std_path(),
            "layer_varsfile=varsfile\nlayer_inline=varsfile\n",
        )
        .unwrap();
        fs::write(
            api_root.join("request.hurl").as_std_path(),
            "# @vars session\nGET https://example.com\n",
        )
        .unwrap();

        let context = resolver
            .resolve_run_context("api", "request")
            .expect("run context");
        let include_result = Includer::new(resolver.clone())
            .merge(context.resolution.file_path.as_path())
            .expect("merge");

        let args = RunArgs {
            exec: ExecutionArgs {
                api: "api".to_string(),
                file: "request".to_string(),
                env: None,
                vars_file: Some(Utf8PathBuf::from("extra.vars")),
                inline_vars: vec![KeyValue {
                    key: "layer_inline".to_string(),
                    value: "inline".to_string(),
                }],
                file_root: None,
                verbosity: 0,
            },
            json_output: None,
            test_mode: false,
            print_only_full_response: false,
            print_only_response_body: false,
            silent: true,
        };

        let env_overrides = reduce_env_overrides(&resolver, &include_result);
        let vars = build_variables(
            &resolver,
            &context,
            &include_result,
            &env_overrides,
            &args,
            true,
        )
        .expect("build variables");

        // The HURL_* process-env layer is not asserted here; doing so would
        // require mutating the process environment.
        assert_eq!(vars.get("layer_global").map(String::as_str), Some("global"));
        assert_eq!(vars.get("layer_file").map(String::as_str), Some("session"));
        assert_eq!(
            vars.get("layer_varsfile").map(String::as_str),
            Some("varsfile")
        );
        assert_eq!(vars.get("layer_inline").map(String::as_str), Some("inline"));
    }
}
