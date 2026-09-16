use camino::Utf8Path;
use hurl::output;
use hurl::runner::{HurlResult, Output as RunnerOutput};
use hurl::util::term::{Stdout, WriteMode};
use hurl_core::input::Input;

use super::OutputError;

/// Renders the JSON report to a string through hurl's buffered terminal,
/// without touching the filesystem.
///
/// # Errors
/// Fails as `StreamWrite` when hurl cannot serialize the report.
pub fn render_json_report(
    result: &HurlResult,
    merged: &str,
    display_path: &str,
) -> Result<String, OutputError> {
    let input = Input::new(display_path);
    let mut stdout = Stdout::new(WriteMode::Buffered);
    output::write_json(result, merged, &input, None, &mut stdout, false).map_err(|source| {
        OutputError::StreamWrite {
            target: "in-memory buffer".to_string(),
            source,
        }
    })?;
    Ok(String::from_utf8_lossy(stdout.buffer()).into_owned())
}

pub fn write_json_report(
    result: &HurlResult,
    merged: &str,
    display_path: &str,
    target: &Utf8Path,
) -> Result<(), OutputError> {
    if target.as_str() == "-" {
        let mut stdout = Stdout::new(WriteMode::Immediate);
        let input = Input::new(display_path);
        output::write_json(result, merged, &input, None, &mut stdout, false).map_err(|source| {
            OutputError::StreamWrite {
                target: "-".to_string(),
                source,
            }
        })?;
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|source| OutputError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let input = Input::new(display_path);
    let mut stdout = Stdout::new(WriteMode::Immediate);
    let runner_output = RunnerOutput::File(target.as_std_path().to_path_buf());
    output::write_json(
        result,
        merged,
        &input,
        Some(&runner_output),
        &mut stdout,
        false,
    )
    .map_err(|source| OutputError::StreamWrite {
        target: target.to_string(),
        source,
    })
}
