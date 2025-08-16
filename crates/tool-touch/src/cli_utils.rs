use crate::models::{TouchArgs, TouchTimeWord};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use clap::{Arg, Command};
use filetime::FileTime;
use shared::command_line::cli_builder::CommandExt;
use std::io;

/// Parses command-line arguments for the touch utility.
///
/// Uses clap for argument parsing and validation. Supports Unix touch
/// command options including timestamps, reference files, and file creation control.
///
/// # Returns
/// Parsed and validated `TouchArgs` structure containing all user options
/// and file list to process.
///
/// # Arguments Supported
/// - `-a`: Change access time only
/// - `-c, --no-create`: Don't create missing files  
/// - `-d, --date`: Parse date string for timestamp
/// - `-m`: Change modification time only
/// - `-r, --reference`: Use reference file's timestamps
/// - `-t`: Use formatted timestamp string
/// - `--time`: Specify which time to change (access/modify)
/// - `files`: List of files to touch
///
/// # Errors
/// Exits with error on invalid arguments, date parsing failures,
/// or reference file access issues.
pub fn get_cli_arguments() -> TouchArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Update the access and modification times of each FILE to the current time.",
            "Mimics the Unix 'touch' command.\
            Update the access and modification times of each FILE to the current time.\n\n\
            A FILE argument that does not exist is created empty, unless -c or -h is supplied.\n\n\
            A FILE argument string of - is handled specially and causes touch to change the times \
            of the file associated with standard output.",
        )
        .arg(
            Arg::new("access")
                .short('a')
                .action(clap::ArgAction::SetTrue)
                .help("Change only the access time"),
        )
        .arg(
            Arg::new("no-create")
                .short('c')
                .long("no-create")
                .action(clap::ArgAction::SetTrue)
                .help("Do not create any files"),
        )
        .arg(
            Arg::new("date")
                .short('d')
                .long("date")
                .value_name("STRING")
                .help("Parse STRING and use it instead of current time"),
        )
        .arg(
            // Yeah, I know. Just keeping it close to the original implementation.
            Arg::new("ignore")
                .short('f')
                .action(clap::ArgAction::SetTrue)
                .help("(ignored)"),
        )
        .arg(
            Arg::new("no-dereference")
                .short('n') // -h is used for help, so I needed to change this.
                .long("no-dereference")
                .action(clap::ArgAction::SetTrue)
                .help("Affect each symbolic link instead of any referenced file"),
        )
        .arg(
            Arg::new("modify")
                .short('m')
                .action(clap::ArgAction::SetTrue)
                .help("Change only the modification time"),
        )
        .arg(
            Arg::new("reference")
                .short('r')
                .long("reference")
                .value_name("FILE")
                .help("Use this file's times instead of current time"),
        )
        .arg(
            Arg::new("time-spec")
                .short('t')
                .value_name("[[CC]YY]MMDDhhmm[.ss]")
                .help("Use specified time instead of current time"),
        )
        .arg(
            Arg::new("time")
                .long("time")
                .value_name("WORD")
                .help("Specify which time to change: access, atime, use, modify, mtime"),
        )
        .arg(
            Arg::new("files")
                .value_name("FILE")
                .action(clap::ArgAction::Append)
                .required(true)
                .help("Files to touch"),
        )
        .get_matches();

    let access = matches.get_flag("access");
    let no_create = matches.get_flag("no-create");
    let ignore = matches.get_flag("ignore");
    let no_dereference = matches.get_flag("no-dereference");
    let modify = matches.get_flag("modify");

    // Convert time argument to TouchTimeWord
    let time = match matches.get_one::<String>("time") {
        Some(time_str) => match time_str.to_lowercase().as_str() {
            "access" | "atime" | "use" => TouchTimeWord::AccessOnly,
            "modify" | "mtime" => TouchTimeWord::ModifyOnly,
            _ => {
                eprintln!("Invalid time specification: {}", time_str);
                std::process::exit(1);
            }
        },
        None => {
            if access && modify {
                TouchTimeWord::AccessAndModify
            } else if access {
                TouchTimeWord::AccessOnly
            } else if modify {
                TouchTimeWord::ModifyOnly
            } else {
                TouchTimeWord::AccessAndModify
            }
        }
    };

    let date: Option<FileTime> = match matches.get_one::<String>("date") {
        Some(date_str) => match parse_date_string(date_str) {
            Ok(date_filetype) => Some(date_filetype),
            Err(_) => {
                eprintln!("Error parsing date string: {}", date_str);
                std::process::exit(1);
            }
        },
        _ => None,
    };

    let time_spec: Option<FileTime> = match matches.get_one::<String>("time-spec") {
        Some(time_spec_str) => match parse_time_spec(time_spec_str) {
            Ok(time_spec_filetime) => Some(time_spec_filetime),
            Err(_) => {
                eprintln!("Error parsing time-spec string: {}", time_spec_str);
                std::process::exit(1);
            }
        },
        _ => None,
    };

    let reference: Option<(FileTime, FileTime)> = match matches.get_one::<String>("reference") {
        Some(reference_str) => match get_reference_times(reference_str, no_dereference) {
            Ok((atime, mtime)) => Some((atime, mtime)),
            Err(_) => {
                eprintln!("Error parsing reference string: {}", reference_str);
                std::process::exit(1);
            }
        },
        _ => None,
    };

    let files: Vec<String> = matches
        .get_many::<String>("files")
        .unwrap_or_default()
        .cloned()
        .collect();

    TouchArgs {
        access,
        no_create,
        date,
        ignore,
        no_dereference,
        modify,
        reference,
        time_spec,
        time,
        files,
    }
}

/// Validates command-line arguments for consistency and completeness.
///
/// Ensures at least one file is specified and only one time source
/// is provided (date, time_spec, or reference).
///
/// # Parameters
/// - `args`: Parsed touch arguments to validate
///
/// # Panics
/// - No files specified
/// - Multiple time sources provided simultaneously
pub fn validate_cli_arguments(args: &TouchArgs) {
    if args.files.is_empty() {
        panic!("No files specified");
    }

    let mut time_sources = 0;

    if args.date.is_some() {
        time_sources += 1;
    }
    if args.time_spec.is_some() {
        time_sources += 1;
    }
    if args.reference.is_some() {
        time_sources += 1;
    }

    if time_sources > 1 {
        panic!("Cannot specify times from more than one source");
    }
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
