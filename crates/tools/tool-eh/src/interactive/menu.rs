//! The menu: a profile, an action, that action's options, a review with the
//! equivalent command line, and a confirm. It only builds the arguments; the
//! normal parse, merge, and checks run on them afterwards.

use crate::cli_utils::format_name;
use crate::interactive::prompter::{Answer, Check, Prompter};
use crate::profile_app::{aligned_names, no_profiles};
use crate::progress::{display, target_label};
use crate::send_cli::DEFAULT_SEND_COUNT;
use anyhow::{Context, anyhow};
use eh_core::connection::{validated_event_hub_name, validated_namespace};
use eh_core::producer::{MAX_SEND_COUNT, MAX_SEND_INTERVAL_MS};
use eh_persistence::config::ToolConfig;
use eh_persistence::export::ExportFormat;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

/// The line for a terminal whose keys can't be read.
pub(crate) const CANNOT_READ_KEYS: &str = "eh can't read keys from this terminal, so the menu \
                                           can't run. In Git Bash, start Git Bash with \
                                           MSYS=enable_pcon.";

/// The line for a workspace whose every profile fails to load.
const NONE_CAN_LOAD: &str = "No profile here can load; eh profile list shows why.";

/// The characters besides ASCII letters and digits that a POSIX shell reads
/// literally outside quotes.
const SHELL_PLAIN: &str = "._/:=@,-";

/// What the menu works from, gathered before the first prompt.
pub(crate) struct MenuContext {
    /// The workspace folder.
    pub root: PathBuf,
    /// The name and `config.json` of every profile that loads, in listing
    /// order.
    pub profiles: Vec<(String, ToolConfig)>,
    /// The names of the profiles that can't load.
    pub cannot_load: Vec<String>,
}

/// How the menu ended.
#[derive(Debug, PartialEq)]
pub(crate) enum MenuOutcome<T> {
    /// Run this command line; its review was printed and confirmed.
    Run(T),
    /// Esc at a prompt, or No at the confirm. Nothing ran.
    Cancelled,
    /// Ctrl-C at a prompt. Nothing ran.
    Interrupted,
    /// There is no profile to offer, or the terminal can't be read; the line
    /// says which. Nothing ran.
    Unavailable(String),
}

/// Why the flow stopped before it built a command line.
enum Stop {
    Cancelled,
    Interrupted,
    Unavailable,
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for Stop {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

#[derive(Clone, Copy)]
enum Action {
    Read,
    Send,
    Export,
}

/// Where a send's body comes from.
#[derive(Clone, Copy)]
enum BodyFrom {
    Typed,
    File,
}

/// One action's part of the command line, and how the review shows it.
struct Planned {
    subcommand: &'static str,
    /// The flag tokens after `--profile NAME`.
    options: Vec<String>,
    /// The review line naming the profile and where the action points.
    review: String,
    /// The confirm question, and its answer on Enter.
    confirm: String,
    default: bool,
}

/// Runs the menu over `context`, printing its notes and the review to `out`.
///
/// # Errors
/// Fails when a prompt fails, when the prompter picks a choice it was not
/// offered, and when `out` can't be written.
pub(crate) fn run(
    prompter: &mut impl Prompter,
    context: &MenuContext,
    out: &mut impl Write,
) -> anyhow::Result<MenuOutcome<Vec<OsString>>> {
    if context.profiles.is_empty() {
        let line = if context.cannot_load.is_empty() {
            no_profiles(&context.root)
        } else {
            NONE_CAN_LOAD.to_owned()
        };
        return Ok(MenuOutcome::Unavailable(line));
    }
    match flow(prompter, context, out) {
        Ok(command) => Ok(MenuOutcome::Run(command)),
        Err(Stop::Cancelled) => Ok(MenuOutcome::Cancelled),
        Err(Stop::Interrupted) => Ok(MenuOutcome::Interrupted),
        Err(Stop::Unavailable) => Ok(MenuOutcome::Unavailable(CANNOT_READ_KEYS.to_owned())),
        Err(Stop::Failed(error)) => Err(error),
    }
}

fn flow(
    prompter: &mut impl Prompter,
    context: &MenuContext,
    out: &mut impl Write,
) -> Result<Vec<OsString>, Stop> {
    if !context.cannot_load.is_empty() {
        note(out, &skipped(&context.cannot_load))?;
    }
    let (name, config) = choose(prompter, "Profile:", &profile_choices(&context.profiles), 0)?;
    let actions = [
        ("read".to_owned(), Action::Read),
        ("send".to_owned(), Action::Send),
        ("export".to_owned(), Action::Export),
    ];
    let planned = match choose(prompter, "Action:", &actions, 0)? {
        Action::Read => plan_read(prompter, name, config, out)?,
        Action::Send => plan_send(prompter, name, config)?,
        Action::Export => plan_export(prompter, name, config)?,
    };
    finish(prompter, name, planned, out)
}

/// One option per profile: its name, padded to the longest, then its target.
fn profile_choices(profiles: &[(String, ToolConfig)]) -> Vec<(String, (&str, &ToolConfig))> {
    aligned_names(profiles.iter().map(|(name, _)| name.as_str()))
        .into_iter()
        .zip(profiles)
        .map(|(shown, (name, config))| {
            (
                format!("{shown}  {}", target_label(&config.target)),
                (name.as_str(), config),
            )
        })
        .collect()
}

fn plan_read(
    prompter: &mut impl Prompter,
    name: &str,
    config: &ToolConfig,
    out: &mut impl Write,
) -> Result<Planned, Stop> {
    let group = prefilled(
        prompter,
        "Consumer group",
        &config.target.consumer_group,
        consumer_group_check,
    )?;
    let mut options = vec![format!("--consumer-group={group}")];
    // A profile's `ignore_checkpoint` can't be turned off by a flag, so it is
    // shown rather than asked.
    if config.capture.ignore_checkpoint {
        note(out, "Start from: Earliest event (set in the profile)")?;
    } else {
        let starts = [
            ("Stored checkpoint".to_owned(), false),
            ("Earliest event".to_owned(), true),
        ];
        if choose(prompter, "Start from:", &starts, 0)? {
            options.push("--ignore-checkpoint".to_owned());
        }
    }
    Ok(Planned {
        subcommand: "read",
        options,
        review: format!(
            "Reads from: {}",
            source(name, &target_label(&config.target))
        ),
        confirm: "Start reading?".to_owned(),
        default: true,
    })
}

/// A send passes its destination as flags: the namespace and event hub the
/// menu read from the profile, or, when the profile lacks either, both typed
/// in. The confirm names that destination, and the flags hold the run to it
/// even if the profile's `config.json` changes before the run reads it.
fn plan_send(
    prompter: &mut impl Prompter,
    name: &str,
    config: &ToolConfig,
) -> Result<Planned, Stop> {
    let (namespace, hub) = match (&config.target.namespace, &config.target.event_hub) {
        (Some(namespace), Some(hub)) => (namespace.clone(), hub.clone()),
        (None, _) | (_, None) => (
            ask(prompter.text(
                "Namespace (e.g. my-ns.servicebus.windows.net):",
                "",
                namespace_check,
            ))?,
            ask(prompter.text("Event hub (e.g. orders):", "", event_hub_check))?,
        ),
    };
    let mut options = vec![
        format!("--namespace={namespace}"),
        format!("--entity-path={hub}"),
    ];
    options.push(body_option(prompter)?);
    let count = ask(prompter.text("Count (empty for 1):", "", count_check))?;
    let count = if count.trim().is_empty() {
        DEFAULT_SEND_COUNT
    } else {
        let count = number(&count)?;
        options.push(format!("--count={count}"));
        count
    };
    let interval = ask(prompter.text(
        "Milliseconds between events (empty for full batches):",
        "",
        interval_check,
    ))?;
    if !interval.trim().is_empty() {
        options.push(format!("--interval-ms={}", number(&interval)?));
    }
    if ask(prompter.confirm("Treat the body as a template?", false))? {
        options.push("--template".to_owned());
    }
    let (namespace, hub) = (display(&namespace), display(&hub));
    let events = if count == 1 { "event" } else { "events" };
    Ok(Planned {
        subcommand: "send",
        options,
        review: format!(
            "Sends to: {}",
            source(name, &format!("namespace {namespace}, event hub {hub}"))
        ),
        confirm: format!("Send {count} {events} to {namespace}/{hub}?"),
        default: false,
    })
}

fn body_option(prompter: &mut impl Prompter) -> Result<String, Stop> {
    let sources = [
        ("Type it".to_owned(), BodyFrom::Typed),
        ("From a file".to_owned(), BodyFrom::File),
    ];
    Ok(match choose(prompter, "Body:", &sources, 0)? {
        BodyFrom::Typed => {
            let body = ask(prompter.text("Body text (one line):", "", one_line_body_check))?;
            format!("--body={body}")
        }
        BodyFrom::File => {
            let path = ask(prompter.text("Body file:", "", body_file_check))?;
            format!("--body-file={}", path.trim())
        }
    })
}

fn plan_export(
    prompter: &mut impl Prompter,
    name: &str,
    config: &ToolConfig,
) -> Result<Planned, Stop> {
    let saved = &config.storage.export;
    let formats = [ExportFormat::Txt, ExportFormat::Csv, ExportFormat::Json]
        .map(|format| (format_name(format).to_owned(), format));
    let start = match saved.format {
        ExportFormat::Txt => 0,
        ExportFormat::Csv => 1,
        ExportFormat::Json => 2,
    };
    let format = choose(prompter, "Format:", &formats, start)?;
    let metadata = ask(prompter.confirm("Include metadata?", saved.include_metadata))?;
    Ok(Planned {
        subcommand: "export",
        options: vec![
            format!("--export-format={}", format_name(format)),
            format!("--include-metadata={metadata}"),
        ],
        review: format!(
            "Exports from: {}",
            source(name, &target_label(&config.target))
        ),
        confirm: "Export now?".to_owned(),
        default: true,
    })
}

/// Prints the review and the equivalent command, then asks the confirm. A No
/// cancels.
fn finish(
    prompter: &mut impl Prompter,
    name: &str,
    planned: Planned,
    out: &mut impl Write,
) -> Result<Vec<OsString>, Stop> {
    let mut tokens = vec![
        "eh".to_owned(),
        planned.subcommand.to_owned(),
        "--profile".to_owned(),
        name.to_owned(),
    ];
    tokens.extend(planned.options);
    note(out, &planned.review)?;
    note(out, &format!("Same as: {}", command_line(&tokens)))?;
    if !ask(prompter.confirm(&planned.confirm, planned.default))? {
        return Err(Stop::Cancelled);
    }
    Ok(tokens.into_iter().map(OsString::from).collect())
}

/// A prompt's value, or the stop it asks for.
fn ask<T>(answer: anyhow::Result<Answer<T>>) -> Result<T, Stop> {
    match answer? {
        Answer::Value(value) => Ok(value),
        Answer::Cancelled => Err(Stop::Cancelled),
        Answer::Interrupted => Err(Stop::Interrupted),
        Answer::Unavailable => Err(Stop::Unavailable),
    }
}

/// The value of the choice picked from `choices`, each shown by its label.
fn choose<T: Copy>(
    prompter: &mut impl Prompter,
    prompt: &str,
    choices: &[(String, T)],
    start: usize,
) -> Result<T, Stop> {
    let labels: Vec<String> = choices.iter().map(|(label, _)| label.clone()).collect();
    let index = ask(prompter.select(prompt, &labels, start))?;
    match choices.get(index) {
        Some((_, value)) => Ok(*value),
        None => Err(Stop::Failed(anyhow!(
            "The menu got choice {} of a {}-item list",
            index + 1,
            choices.len()
        ))),
    }
}

/// A text answer starting from the `saved` value. A saved value the terminal
/// would show escaped starts empty instead, so an escaped copy is never sent.
fn prefilled(
    prompter: &mut impl Prompter,
    label: &str,
    saved: &str,
    check: Check,
) -> Result<String, Stop> {
    if display(saved) == saved {
        ask(prompter.text(&format!("{label}:"), saved, check))
    } else {
        let prompt = format!("{label} (the saved value has characters that can't be shown):");
        ask(prompter.text(&prompt, "", check))
    }
}

/// A number a check already accepted.
fn number(text: &str) -> Result<u64, Stop> {
    text.trim()
        .parse()
        .with_context(|| format!("The menu took '{}' as a number", display(text)))
        .map_err(Stop::Failed)
}

fn note(out: &mut impl Write, line: &str) -> anyhow::Result<()> {
    writeln!(out, "{line}").context("Failed to print the menu's notes")
}

fn source(name: &str, target: &str) -> String {
    format!("profile {}, {target}", display(name))
}

fn skipped(names: &[String]) -> String {
    let listed: Vec<String> = names.iter().map(display).collect();
    let profiles = if names.len() == 1 {
        "profile"
    } else {
        "profiles"
    };
    format!(
        "Skipped {} {profiles} that can't load ({}); eh profile list shows why.",
        names.len(),
        listed.join(", ")
    )
}

/// `tokens` as one line a POSIX shell reads back as the same arguments,
/// each token escaped for the terminal first.
fn command_line(tokens: &[String]) -> String {
    let quoted: Vec<String> = tokens
        .iter()
        .map(|token| shell_quoted(&display(token)))
        .collect();
    quoted.join(" ")
}

fn shell_quoted(token: &str) -> String {
    let plain = !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || SHELL_PLAIN.contains(c));
    if plain {
        token.to_owned()
    } else {
        format!("'{}'", token.replace('\'', r"'\''"))
    }
}

/// Whether `input` shows on the terminal exactly as it is; the "Same as"
/// command shows every value escaped, so only such a value prints as it runs.
fn shown_as_typed(input: &str) -> bool {
    display(input) == input
}

fn consumer_group_check(input: &str) -> Result<(), String> {
    if input.trim().is_empty() {
        Err("Type a consumer group, such as $Default".to_owned())
    } else if !shown_as_typed(input) {
        Err("The consumer group has characters that can't be shown".to_owned())
    } else {
        Ok(())
    }
}

fn namespace_check(input: &str) -> Result<(), String> {
    validated_namespace(input)
        .map(drop)
        .map_err(|error| format!("{error:#}"))
}

fn event_hub_check(input: &str) -> Result<(), String> {
    validated_event_hub_name(input)
        .map(drop)
        .map_err(|error| format!("{error:#}"))
}

/// A typed body. A pasted line break or another control character would run
/// as typed but print escaped, so such a body goes in a file instead.
fn one_line_body_check(input: &str) -> Result<(), String> {
    if shown_as_typed(input) {
        Ok(())
    } else {
        Err(
            "The body has line breaks or characters that can't be shown; pick From a file"
                .to_owned(),
        )
    }
}

/// A body file path; `-` would read the body from the terminal, so it is
/// refused.
fn body_file_check(input: &str) -> Result<(), String> {
    match input.trim() {
        "" => Err("Type the body file's path".to_owned()),
        "-" => Err("The menu can't read a body from stdin; type a file path".to_owned()),
        _ if !shown_as_typed(input) => {
            Err("The path has characters that can't be shown".to_owned())
        }
        _ => Ok(()),
    }
}

fn count_check(input: &str) -> Result<(), String> {
    if input.trim().is_empty() {
        Ok(())
    } else {
        in_range(input, MAX_SEND_COUNT)
    }
}

fn interval_check(input: &str) -> Result<(), String> {
    if input.trim().is_empty() {
        Ok(())
    } else {
        in_range(input, MAX_SEND_INTERVAL_MS)
    }
}

fn in_range(input: &str, max: u64) -> Result<(), String> {
    match input.trim().parse::<u64>() {
        Ok(value) if (1..=max).contains(&value) => Ok(()),
        Ok(_) | Err(_) => Err(format!("Type a whole number from 1 to {max}")),
    }
}

#[cfg(test)]
mod tests;
