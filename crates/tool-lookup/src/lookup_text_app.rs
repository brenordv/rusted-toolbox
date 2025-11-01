use crate::lookup_shared::{list_files, normalize_extensions, path_matches_allowed};
use crate::models::TextLookupConfig;
use anyhow::{anyhow, Result};
use shared::constants::general::DASH_LINE;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;
use tracing::error;

pub fn print_header(args: &TextLookupConfig) {
    println!("Lookup v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("Text: {}", args.text);
    println!("Path: {}", args.path);
    println!("File extensions: {:?}", args.file_extensions);
    if args.current_only {
        println!("Search Mode: Current folder only")
    } else {
        println!("Search Mode: Recursive")
    }
    println!("Print Line data only: {}", args.line_only);
}

pub fn run_text_lookup(config: &TextLookupConfig) -> Result<()> {
    let start = Instant::now();

    let base_path = PathBuf::from(&config.path);
    if !base_path.exists() {
        let err_msg = format!("Path does not exist: {}", base_path.display());
        error!("{}", err_msg);
        return Err(anyhow!(err_msg));
    }

    let normalized_extensions = normalize_extensions(&config.file_extensions);
    let needle = config.text.to_ascii_lowercase();

    let files_iter = list_files(&base_path, config.current_only)?;
    let mut files_read: u64 = 0;
    let mut total_lines: u64 = 0;
    let mut matches_found: u64 = 0;

    for file_path in files_iter {
        if !path_matches_allowed(&file_path, &normalized_extensions) {
            continue;
        }

        let file = match File::open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open file '{}': {}", file_path.display(), e);
                continue;
            }
        };
        files_read += 1;

        let reader = BufReader::new(file);
        for (idx, line_res) in reader.lines().enumerate() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue, // Skip problematic lines
            };
            total_lines += 1;

            if line.to_ascii_lowercase().contains(&needle) {
                matches_found += 1;
                if config.line_only {
                    println!("{}", line);
                } else {
                    println!("{}:{}| {}", file_path.display(), idx + 1, line);
                }
            }
        }
    }

    if !config.no_header {
        let elapsed = start.elapsed();
        eprintln!(
            "Searched in {} files, {} lines, {} matches. Took {:?}.",
            files_read, total_lines, matches_found, elapsed
        );
    }

    Ok(())
}