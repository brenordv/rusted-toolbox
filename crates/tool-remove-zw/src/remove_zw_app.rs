use crate::models::{InputSource, OutputTarget, RemoveZwArgs};
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use shared::constants::general::SIZE_8KB;

static FORMAT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\p{Cf}").unwrap());

pub fn run(args: &RemoveZwArgs) -> Result<()> {
    let expanded_inputs = expand_inputs(args)?;

    for input in expanded_inputs {
        match input {
            InputSource::Stdin => {
                let content = read_stdin().context("Failed to read from stdin")?;
                process_and_write(args, &InputSource::Stdin, &content)?;
            }
            InputSource::File(path) => {
                if should_skip_by_extension(&path, &args.extensions) {
                    if args.verbose {
                        eprintln!(
                            "remove-zw: skipping '{}' (extension filter)",
                            path.display()
                        );
                    }
                    continue;
                }

                if args.extensions.is_empty() && is_likely_binary(&path)? {
                    if args.verbose {
                        eprintln!("remove-zw: skipping binary file '{}'", path.display());
                    }
                    continue;
                }

                let content = match read_file_text(&path) {
                    Ok(content) => content,
                    Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                        if args.verbose {
                            eprintln!("remove-zw: skipping binary file '{}'", path.display());
                        }
                        continue;
                    }
                    Err(err) => {
                        return Err(anyhow!(
                            "Failed to read file '{}': {}",
                            path.display(),
                            err
                        ));
                    }
                };

                process_and_write(args, &InputSource::File(path), &content)?;
            }
            InputSource::Directory(_) => {
                return Err(anyhow!(
                    "Internal error: directories should have been expanded"
                ));
            }
        }
    }

    Ok(())
}

fn process_and_write(args: &RemoveZwArgs, input: &InputSource, content: &str) -> Result<()> {
    let source_label = match input {
        InputSource::Stdin => "stdin".to_string(),
        InputSource::File(path) => path.display().to_string(),
        InputSource::Directory(path) => path.display().to_string(),
    };

    let (cleaned, removed) = strip_format_chars(content);

    if args.verbose {
        eprintln!(
            "remove-zw: {} -> removed {} zero-width chars",
            source_label, removed
        );
    }

    write_output(args, input, cleaned.as_ref())
        .with_context(|| format!("Failed to write output for {}", source_label))?;

    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("Failed to read stdin")?;
    Ok(buffer)
}

fn write_output(args: &RemoveZwArgs, input: &InputSource, content: &str) -> Result<()> {
    if args.in_place {
        if let InputSource::File(path) = input {
            return write_in_place(path, content);
        }
        return Err(anyhow!("Cannot use --in-place with stdin"));
    }

    if let Some(output) = &args.output {
        return match output {
            OutputTarget::Stdout => write_stdout(content),
            OutputTarget::File(path) => write_file(path, content),
        };
    }

    match input {
        InputSource::Stdin => write_stdout(content),
        InputSource::File(path) => {
            let output_path = build_output_path(path);
            write_file(&output_path, content)
        }
        InputSource::Directory(_) => Err(anyhow!(
            "Internal error: directories should have been expanded"
        )),
    }
}

fn write_stdout(content: &str) -> Result<()> {
    let mut stdout = io::stdout();
    stdout
        .write_all(content.as_bytes())
        .context("Failed to write to stdout")?;
    stdout.flush().context("Failed to flush stdout")?;
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content)
        .with_context(|| format!("Failed to write file '{}'", path.display()))
}

fn write_in_place(path: &Path, content: &str) -> Result<()> {
    let temp_path = build_temp_path(path);

    fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write temp file '{}'", temp_path.display()))?;

    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("Failed to remove original file '{}'", path.display()))?;
    }

    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "Failed to replace '{}' with temp file: {}",
            path.display(),
            err
        ));
    }

    Ok(())
}

fn build_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| Cow::Borrowed("output"));
    let temp_name = format!("{}.remove-zw.tmp", file_name);
    path.with_file_name(temp_name)
}

fn build_output_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_else(|| Cow::Borrowed("output"));

    let new_name = match path.extension().map(|ext| ext.to_string_lossy()) {
        Some(ext) if !ext.is_empty() => format!("{}.cleaned.{}", stem, ext),
        _ => format!("{}.cleaned", stem),
    };

    path.with_file_name(new_name)
}

fn strip_format_chars(input: &str) -> (Cow<'_, str>, usize) {
    let removed = FORMAT_RE.find_iter(input).count();
    if removed == 0 {
        return (Cow::Borrowed(input), 0);
    }

    let cleaned = FORMAT_RE.replace_all(input, "");
    (cleaned, removed)
}

fn expand_inputs(args: &RemoveZwArgs) -> Result<Vec<InputSource>> {
    let mut expanded = Vec::new();

    for input in &args.inputs {
        match input {
            InputSource::Stdin => expanded.push(InputSource::Stdin),
            InputSource::File(path) => expanded.push(InputSource::File(path.clone())),
            InputSource::Directory(path) => {
                let files = collect_files_in_dir(path, args.recursive, &args.extensions)?;
                for file in files {
                    expanded.push(InputSource::File(file));
                }
            }
        }
    }

    Ok(expanded)
}

fn collect_files_in_dir(
    root: &Path,
    recursive: bool,
    extensions: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)
            .with_context(|| format!("Failed to read directory '{}'", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                if recursive {
                    stack.push(path);
                }
                continue;
            }

            if file_type.is_file() && !should_skip_by_extension(&path, extensions) {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn should_skip_by_extension(path: &Path, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return false;
    }

    let ext = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => ext.to_lowercase(),
        None => return true,
    };

    !extensions.iter().any(|allowed| allowed == &ext)
}

fn read_file_text(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

fn is_likely_binary(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file '{}'", path.display()))?;
    let mut buffer = vec![0u8; SIZE_8KB];
    let read = file
        .read(&mut buffer)
        .with_context(|| format!("Failed to read file '{}'", path.display()))?;

    if read == 0 {
        return Ok(false);
    }

    let sample = &buffer[..read];
    if sample.iter().any(|byte| *byte == 0) {
        return Ok(true);
    }

    Ok(std::str::from_utf8(sample).is_err())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_format_chars() {
        let input = "hello\u{200B}world\u{200C}";
        let (cleaned, removed) = strip_format_chars(input);
        assert_eq!(cleaned, "helloworld");
        assert_eq!(removed, 2);
    }

    #[test]
    fn leaves_non_format_chars() {
        let input = "hello world";
        let (cleaned, removed) = strip_format_chars(input);
        assert_eq!(cleaned, "hello world");
        assert_eq!(removed, 0);
    }

    #[test]
    fn builds_output_path_with_extension() {
        let path = Path::new("sample.txt");
        let output = build_output_path(path);
        assert_eq!(output, PathBuf::from("sample.cleaned.txt"));
    }

    #[test]
    fn builds_output_path_without_extension() {
        let path = Path::new("sample");
        let output = build_output_path(path);
        assert_eq!(output, PathBuf::from("sample.cleaned"));
    }
}
