use std::collections::HashMap;
use std::io::{self, Write};

use camino::{Utf8Path, Utf8PathBuf};
use hurl::runner::HurlResult;
use hurl_core::error::DisplaySourceError;

use crate::includer::IncludeResult;

use super::OutputError;

const CONTEXT_LINES: u32 = 2;

pub fn print_test_summary<W: Write>(
    writer: &mut W,
    result: &HurlResult,
    includes: &IncludeResult,
    requests_root: &Utf8Path,
) -> Result<(), OutputError> {
    let total = result.entries.len();
    let failed = result
        .entries
        .iter()
        .filter(|entry| !entry.errors.is_empty())
        .count();
    let passed = total - failed;

    writeln!(
        writer,
        "Summary: {passed}/{total} entries passed ({} failed)",
        failed
    )
    .map_err(|source| OutputError::StreamWrite {
        target: "-".to_string(),
        source,
    })?;

    let mut cache: HashMap<Utf8PathBuf, Vec<String>> = HashMap::new();

    for entry in &result.entries {
        if entry.errors.is_empty() {
            writeln!(
                writer,
                "  ✓ Entry #{:>3} ({} requests)",
                entry.entry_index,
                entry.calls.len()
            )
            .map_err(|source| OutputError::StreamWrite {
                target: "-".to_string(),
                source,
            })?;
            continue;
        }

        writeln!(
            writer,
            "  ✗ Entry #{:>3} ({} error{})",
            entry.entry_index,
            entry.errors.len(),
            if entry.errors.len() == 1 { "" } else { "s" }
        )
        .map_err(|source| OutputError::StreamWrite {
            target: "-".to_string(),
            source,
        })?;

        for error in &entry.errors {
            let description = error.description();
            writeln!(writer, "      - {description}").map_err(|source| {
                OutputError::StreamWrite {
                    target: "-".to_string(),
                    source,
                }
            })?;

            if let Some(mapping) = includes.map_source(&error.source_info) {
                let display_path = format_path(requests_root, &mapping.source);
                writeln!(writer, "        at {}:{}", display_path, mapping.line).map_err(
                    |source| OutputError::StreamWrite {
                        target: "-".to_string(),
                        source,
                    },
                )?;

                let snippet = load_snippet(&mut cache, &mapping.source)?;
                if !snippet.is_empty() {
                    write_snippet(writer, &snippet, mapping.line).map_err(|source| {
                        OutputError::StreamWrite {
                            target: "-".to_string(),
                            source,
                        }
                    })?;
                }
            }
        }
    }

    Ok(())
}

fn load_snippet<'a>(
    cache: &'a mut HashMap<Utf8PathBuf, Vec<String>>,
    path: &Utf8Path,
) -> Result<Vec<String>, OutputError> {
    if let Some(lines) = cache.get(path) {
        return Ok(lines.clone());
    }

    let contents = std::fs::read_to_string(path).map_err(|source| OutputError::SourceRead {
        path: path.to_path_buf(),
        source,
    })?;

    let lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();
    cache.insert(path.to_path_buf(), lines.clone());
    Ok(lines)
}

fn write_snippet<W: Write>(
    writer: &mut W,
    lines: &[String],
    line_number: u32,
) -> Result<(), io::Error> {
    let total_lines = lines.len();
    if total_lines == 0 {
        return Ok(());
    }

    let target_line = (line_number as usize).clamp(1, total_lines);
    let mut start_line = target_line.saturating_sub(CONTEXT_LINES as usize);
    if start_line == 0 {
        start_line = 1;
    }
    let end_line = (target_line + CONTEXT_LINES as usize).min(total_lines);

    for current_line_no in start_line..=end_line {
        let marker = if current_line_no == target_line {
            '>'
        } else {
            ' '
        };

        writeln!(
            writer,
            "        {marker} {:>4} | {}",
            current_line_no,
            lines
                .get(current_line_no - 1)
                .map(|s| s.as_str())
                .unwrap_or("")
        )?;
    }

    Ok(())
}

fn format_path(root: &Utf8Path, path: &Utf8Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => relative.to_string(),
        Err(_) => path.to_string(),
    }
}
