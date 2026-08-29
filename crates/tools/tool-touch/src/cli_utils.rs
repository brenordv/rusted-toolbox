use crate::models::{TouchArgs, TouchTimeWord};
use anyhow::{anyhow, Result};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use filetime::FileTime;
use std::io;

/// Update the access and modification times of each FILE to the current time.
///
/// Mimics the Unix 'touch' command. A FILE argument that does not exist is created empty, unless
/// -c is supplied. A FILE argument of - is handled specially and refers to standard output.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Change only the access time
    #[arg(short = 'a')]
    pub access: bool,

    /// Do not create any files
    #[arg(short = 'c', long = "no-create")]
    pub no_create: bool,

    /// Parse STRING and use it instead of the current time
    #[arg(short = 'd', long = "date", value_name = "STRING")]
    pub date: Option<String>,

    /// (ignored)
    #[arg(short = 'f')]
    pub ignore: bool,

    /// Affect each symbolic link instead of any referenced file
    #[arg(short = 'n', long = "no-dereference")]
    pub no_dereference: bool,

    /// Change only the modification time
    #[arg(short = 'm')]
    pub modify: bool,

    /// Use this file's times instead of current time
    #[arg(short = 'r', long = "reference", value_name = "FILE")]
    pub reference: Option<String>,

    /// Use specified time instead of current time
    #[arg(short = 't', value_name = "[[CC]YY]MMDDhhmm[.ss]")]
    pub time_spec: Option<String>,

    /// Specify which time to change: access, atime, use, modify, mtime
    #[arg(long = "time", value_name = "WORD")]
    pub time: Option<String>,

    /// Files to touch
    #[arg(value_name = "FILE", required = true, num_args = 1..)]
    pub files: Vec<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
///
/// # Errors
/// Returns an error for an invalid `--time` word, an unparseable `--date` or `-t` time spec,
/// an inaccessible `--reference` file, or when more than one time source is provided.
pub fn initialize() -> Result<TouchArgs> {
    let args = CliArgs::parse();

    let config = build_touch_args(&args)?;

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        None::<fn()>,
    );

    Ok(config)
}

/// Validates the parsed arguments and resolves them into a [`TouchArgs`].
fn build_touch_args(args: &CliArgs) -> Result<TouchArgs> {
    let time = match &args.time {
        Some(time_str) => match time_str.to_lowercase().as_str() {
            "access" | "atime" | "use" => TouchTimeWord::AccessOnly,
            "modify" | "mtime" => TouchTimeWord::ModifyOnly,
            other => return Err(anyhow!("Invalid time specification: {}", other)),
        },
        None => {
            if args.access && args.modify {
                TouchTimeWord::AccessAndModify
            } else if args.access {
                TouchTimeWord::AccessOnly
            } else if args.modify {
                TouchTimeWord::ModifyOnly
            } else {
                TouchTimeWord::AccessAndModify
            }
        }
    };

    let date = match &args.date {
        Some(date_str) => Some(
            parse_date_string(date_str)
                .map_err(|_| anyhow!("Error parsing date string: {}", date_str))?,
        ),
        None => None,
    };

    let time_spec = match &args.time_spec {
        Some(time_spec_str) => Some(
            parse_time_spec(time_spec_str)
                .map_err(|_| anyhow!("Error parsing time-spec string: {}", time_spec_str))?,
        ),
        None => None,
    };

    let reference = match &args.reference {
        Some(reference_str) => Some(
            get_reference_times(reference_str, args.no_dereference)
                .map_err(|_| anyhow!("Error parsing reference string: {}", reference_str))?,
        ),
        None => None,
    };

    let time_sources = [date.is_some(), time_spec.is_some(), reference.is_some()]
        .iter()
        .filter(|&&provided| provided)
        .count();

    if time_sources > 1 {
        return Err(anyhow!("Cannot specify times from more than one source"));
    }

    Ok(TouchArgs {
        access: args.access,
        no_create: args.no_create,
        date,
        ignore: args.ignore,
        no_dereference: args.no_dereference,
        modify: args.modify,
        reference,
        time_spec,
        time,
        files: args.files.clone(),
    })
}

fn parse_date_string(date_str: &str) -> Result<FileTime, String> {
    // Try various date formats
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%m/%d/%Y",
        "%d %b %Y %H:%M:%S",
        "%d %b %Y %H:%M",
        "%d %b %Y",
        "%a, %d %b %Y %H:%M:%S %z",
    ];

    for format in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(date_str, format) {
            let timestamp = Local
                .from_local_datetime(&dt)
                .single()
                .unwrap_or_else(|| Utc.from_utc_datetime(&dt).with_timezone(&Local));
            return Ok(FileTime::from_unix_time(timestamp.timestamp(), 0));
        }
    }

    // Handle relative times like "now", "yesterday", etc.
    match date_str.to_lowercase().as_str() {
        "now" => Ok(FileTime::now()),
        _ => Err(format!("Invalid date format: {}", date_str)),
    }
}

fn parse_time_spec(time_spec: &str) -> Result<FileTime, String> {
    // Parse format: [[CC]YY]MMDDhhmm[.ss]
    let len = time_spec.len();

    if len < 8 {
        return Err("Time specification too short".to_string());
    }

    let (time_part, seconds) = if time_spec.contains('.') {
        let parts: Vec<&str> = time_spec.split('.').collect();
        if parts.len() != 2 || parts[1].len() != 2 {
            return Err("Invalid seconds format".to_string());
        }
        (
            parts[0],
            parts[1].parse::<u32>().map_err(|_| "Invalid seconds")?,
        )
    } else {
        (time_spec, 0)
    };

    let (year, month, day, hour, minute) = match time_part.len() {
        8 => {
            // MMDDhhmm (current century)
            let now = Local::now();
            let century = (now.year() / 100) * 100;
            let mm = &time_part[0..2];
            let dd = &time_part[2..4];
            let hh = &time_part[4..6];
            let min = &time_part[6..8];
            (century + (now.year() % 100), mm, dd, hh, min)
        }
        10 => {
            // YYMMDDhhmm
            let yy = time_part[0..2].parse::<i32>().map_err(|_| "Invalid year")?;
            let year = if yy >= 70 { 1900 + yy } else { 2000 + yy };
            let mm = &time_part[2..4];
            let dd = &time_part[4..6];
            let hh = &time_part[6..8];
            let min = &time_part[8..10];
            (year, mm, dd, hh, min)
        }
        12 => {
            // CCYYMMDDhhmm
            let ccyy = time_part[0..4].parse::<i32>().map_err(|_| "Invalid year")?;
            let mm = &time_part[4..6];
            let dd = &time_part[6..8];
            let hh = &time_part[8..10];
            let min = &time_part[10..12];
            (ccyy, mm, dd, hh, min)
        }
        _ => return Err("Invalid time specification length".to_string()),
    };

    let month = month.parse::<u32>().map_err(|_| "Invalid month")?;
    let day = day.parse::<u32>().map_err(|_| "Invalid day")?;
    let hour = hour.parse::<u32>().map_err(|_| "Invalid hour")?;
    let minute = minute.parse::<u32>().map_err(|_| "Invalid minute")?;

    let naive_date = NaiveDate::from_ymd_opt(year, month, day).ok_or("Invalid date")?;
    let naive_datetime = naive_date
        .and_hms_opt(hour, minute, seconds)
        .ok_or("Invalid time")?;
    let datetime = Local
        .from_local_datetime(&naive_datetime)
        .single()
        .ok_or("Invalid date/time")?;

    Ok(FileTime::from_unix_time(datetime.timestamp(), 0))
}

fn get_reference_times(
    ref_file: &str,
    no_dereference: bool,
) -> Result<(FileTime, FileTime), io::Error> {
    let metadata = if no_dereference {
        std::fs::symlink_metadata(ref_file)?
    } else {
        std::fs::metadata(ref_file)?
    };

    let atime = FileTime::from_last_access_time(&metadata);
    let mtime = FileTime::from_last_modification_time(&metadata);

    Ok((atime, mtime))
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
    fn access_and_modify_default_to_both() {
        let args = CliArgs::try_parse_from(["touch", "file.txt"]).unwrap();
        let config = build_touch_args(&args).unwrap();
        assert!(matches!(config.time, TouchTimeWord::AccessAndModify));
    }

    #[test]
    fn access_flag_selects_access_only() {
        let args = CliArgs::try_parse_from(["touch", "-a", "file.txt"]).unwrap();
        let config = build_touch_args(&args).unwrap();
        assert!(matches!(config.time, TouchTimeWord::AccessOnly));
    }

    #[test]
    fn invalid_time_word_is_rejected() {
        let args = CliArgs::try_parse_from(["touch", "--time", "bogus", "file.txt"]).unwrap();
        assert!(build_touch_args(&args).is_err());
    }

    #[test]
    fn multiple_time_sources_are_rejected() {
        let args = CliArgs::try_parse_from([
            "touch",
            "-d",
            "2024-01-01",
            "-t",
            "202401011200",
            "file.txt",
        ])
        .unwrap();
        assert!(build_touch_args(&args).is_err());
    }
}
