use crate::models::{TouchArgs, TouchTimeWord};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
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

    /// Parse STRING and use it instead of the current time. Accepts the POSIX
    /// form YYYY-MM-DDThh:mm:SS[.frac][Z] ('T' or a space; 'Z' means UTC),
    /// the ISO offset form YYYY-MM-DDThh:mm:ss[.frac]+hh[:]mm (fraction with
    /// '.' or ','), US month-first MM/DD/YYYY, named months (15 Jan 2024),
    /// RFC-2822 style with offset, and 'now'; date-only values resolve to
    /// local midnight
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

    // Boot logging before validation: the entrypoint reports argument errors
    // through `tracing::error!`, which needs the subscriber installed first or
    // an invalid `-d`/`-t`/`--time` value exits with no message at all.
    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        None::<fn()>,
    );

    build_touch_args(&args)
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
        Some(date_str) => Some(parse_date_string(date_str).map_err(|error| anyhow!(error))?),
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
    // Offset-carrying formats parse offset-aware, so `+0900` (or `+0000`)
    // names the instant it says instead of being read as local wall time.
    // `%z` accepts `+0900` and `+09:00` and tolerates a leading space, and
    // `%.f` makes a dot fraction optional, so the first format covers the
    // T-separated ISO forms with or without a fraction or colon.
    let offset_formats = [
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S %z",
        "%a, %d %b %Y %H:%M:%S %z",
    ];

    let (normalized, utc) = normalize_posix_datetime(date_str);

    for format in offset_formats {
        if let Ok(dt) = DateTime::parse_from_str(date_str, format) {
            return Ok(filetime_from_timestamp(
                dt.timestamp(),
                dt.timestamp_subsec_nanos(),
            ));
        }
        // The comma-fraction convention also applies to the offset forms
        // (`10:30:45,5+0900`). A Z-stripped body carries no offset, so the
        // `utc` flag stays with the naive path below.
        if !utc && normalized != date_str {
            if let Ok(dt) = DateTime::parse_from_str(&normalized, format) {
                return Ok(filetime_from_timestamp(
                    dt.timestamp(),
                    dt.timestamp_subsec_nanos(),
                ));
            }
        }
    }

    // Formats that carry both a date and a time component. `%.f` also matches
    // when no fraction is present, so it covers the whole-second forms too.
    let datetime_formats = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%d %b %Y %H:%M:%S",
        "%d %b %Y %H:%M",
    ];

    // Date-only formats resolve to midnight local time. They parse the input
    // as given: the `Z`/comma conventions belong to the datetime forms only.
    let date_formats = ["%Y-%m-%d", "%m/%d/%Y", "%d %b %Y"];

    let parsed = datetime_formats
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(&normalized, format).ok())
        .map(|dt| (dt, utc))
        .or_else(|| {
            date_formats.iter().find_map(|format| {
                NaiveDate::parse_from_str(date_str, format)
                    .ok()
                    .and_then(|date| date.and_hms_opt(0, 0, 0))
                    .map(|dt| (dt, false))
            })
        });

    if let Some((dt, as_utc)) = parsed {
        let timestamp = if as_utc {
            Utc.from_utc_datetime(&dt).with_timezone(&Local)
        } else {
            Local
                .from_local_datetime(&dt)
                .single()
                .unwrap_or_else(|| Utc.from_utc_datetime(&dt).with_timezone(&Local))
        };
        return Ok(filetime_from_timestamp(
            timestamp.timestamp(),
            timestamp.timestamp_subsec_nanos(),
        ));
    }

    // Handle relative times like "now".
    match date_str.to_lowercase().as_str() {
        "now" => Ok(FileTime::now()),
        _ => Err(format!(
            "Invalid date string: {date_str}. Accepted forms: 'YYYY-MM-DD [hh:mm[:ss[.frac]][Z]]' \
             ('T' also separates date and time), 'YYYY-MM-DDThh:mm:ss[.frac]+hh[:]mm', \
             'MM/DD/YYYY [hh:mm[:ss]]', 'DD Mon YYYY [hh:mm[:ss]]', RFC-2822 style with \
             offset, or 'now'."
        )),
    }
}

/// Applies the POSIX `-d` lexical conventions the chrono format strings can't
/// express: one trailing `Z` right after a digit means UTC, and a `,` between
/// two digits separates the fraction, same as `.`. Everything else, including
/// the `,` in an RFC-2822 style day name, passes through untouched.
fn normalize_posix_datetime(date_str: &str) -> (String, bool) {
    let (body, utc) = match date_str.strip_suffix('Z') {
        Some(body) if body.ends_with(|c: char| c.is_ascii_digit()) => (body, true),
        _ => (date_str, false),
    };

    // `,` and `.` are both one byte, so the swap keeps every char boundary.
    // The neighbor lookups read single bytes; a multibyte neighbor shows up as
    // a non-digit continuation byte and leaves the comma alone.
    let bytes = body.as_bytes();
    let normalized = body
        .char_indices()
        .map(|(index, character)| {
            let digit_before = index > 0 && bytes[index - 1].is_ascii_digit();
            let digit_after = bytes.get(index + 1).is_some_and(|b| b.is_ascii_digit());
            if character == ',' && digit_before && digit_after {
                '.'
            } else {
                character
            }
        })
        .collect();

    (normalized, utc)
}

/// Builds a [`FileTime`] from a second/nanosecond pair, masking the nanosecond
/// overflow chrono uses to encode a parsed leap second (`:60` arrives as
/// nanos >= 1_000_000_000, which `FileTime` must not receive raw).
fn filetime_from_timestamp(seconds: i64, nanos: u32) -> FileTime {
    FileTime::from_unix_time(seconds, nanos % 1_000_000_000)
}

fn parse_time_spec(time_spec: &str) -> Result<FileTime, String> {
    // Parse format: [[CC]YY]MMDDhhmm[.ss]
    // The format only allows ASCII digits and an optional dot; rejecting anything
    // else up front also keeps the byte-index slicing below on char boundaries.
    if !time_spec.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err("Time specification must contain only digits and an optional '.'".to_string());
    }

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
            // YYMMDDhhmm; POSIX century pivot: 69-99 is 19xx, 00-68 is 20xx.
            let yy = time_part[0..2].parse::<i32>().map_err(|_| "Invalid year")?;
            let year = if yy >= 69 { 1900 + yy } else { 2000 + yy };
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
    use chrono::{DateTime, Timelike};
    use clap::CommandFactory;

    /// Converts a [`FileTime`] back into a local datetime so tests can assert
    /// on calendar fields regardless of the machine's timezone.
    fn to_local(ft: FileTime) -> DateTime<Local> {
        Local
            .timestamp_opt(ft.unix_seconds(), 0)
            .single()
            .expect("timestamp converts to a local datetime")
    }

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

    #[test]
    fn time_spec_eight_digits_uses_current_year() {
        let dt = to_local(parse_time_spec("01021530").unwrap());
        assert_eq!(
            (dt.month(), dt.day(), dt.hour(), dt.minute(), dt.second()),
            (1, 2, 15, 30, 0)
        );
    }

    #[test]
    fn time_spec_ten_digits_pivots_century_at_sixty_nine() {
        // POSIX: YY 69-99 resolves to 19xx, 00-68 to 20xx.
        for (input, year) in [
            ("6901021530", 1969),
            ("6801021530", 2068),
            ("7001021530", 1970),
            ("2501021530", 2025),
        ] {
            let dt = to_local(parse_time_spec(input).unwrap());
            assert_eq!(
                (dt.year(), dt.month(), dt.day(), dt.hour(), dt.minute()),
                (year, 1, 2, 15, 30),
                "input: {input}"
            );
        }
    }

    #[test]
    fn time_spec_twelve_digits_parses_full_year() {
        let dt = to_local(parse_time_spec("202512251530").unwrap());
        assert_eq!(
            (dt.year(), dt.month(), dt.day(), dt.hour(), dt.minute()),
            (2025, 12, 25, 15, 30)
        );
    }

    #[test]
    fn time_spec_accepts_seconds_suffix() {
        let dt = to_local(parse_time_spec("202512251530.45").unwrap());
        assert_eq!(dt.second(), 45);
    }

    #[test]
    fn time_spec_rejects_invalid_lengths() {
        assert!(parse_time_spec("0102153").is_err()); // 7 digits
        assert!(parse_time_spec("010215301").is_err()); // 9 digits
        assert!(parse_time_spec("20251225153").is_err()); // 11 digits
    }

    #[test]
    fn time_spec_rejects_out_of_range_seconds() {
        assert!(parse_time_spec("202512251530.61").is_err());
    }

    #[test]
    fn time_spec_rejects_multibyte_input() {
        // 8 and 10 bytes long: the lengths the parser slices by byte index.
        assert!(parse_time_spec("1204\u{20AC}0").is_err());
        assert!(parse_time_spec("1231120\u{20AC}").is_err());
    }

    #[test]
    fn date_string_parses_each_supported_local_format() {
        // Inputs with no offset and no Z are local wall time; asserting the
        // local calendar fields is timezone-independent for those.
        let inputs = [
            "2024-01-15 10:30:45",
            "2024-01-15T10:30:45",
            "2024-01-15 10:30:45.25",
            "2024-01-15T10:30:45,25",
            "2024-01-15 10:30",
            "2024-01-15T10:30",
            "2024-01-15",
            "01/15/2024 10:30:45",
            "01/15/2024 10:30",
            "01/15/2024",
            "15 Jan 2024 10:30:45",
            "15 Jan 2024 10:30",
            "15 Jan 2024",
        ];
        for input in inputs {
            let ft =
                parse_date_string(input).unwrap_or_else(|e| panic!("'{input}' should parse: {e}"));
            let dt = to_local(ft);
            assert_eq!(
                (dt.year(), dt.month(), dt.day()),
                (2024, 1, 15),
                "input: {input}"
            );
        }
    }

    #[test]
    fn date_string_honors_utc_and_offsets_by_exact_epoch() {
        // 2024-01-15T10:30:45Z is 1705314645; epoch assertions pin the UTC
        // interpretation independent of the machine timezone.
        const EPOCH: i64 = 1_705_314_645;

        for input in [
            "2024-01-15T10:30:45Z",
            "2024-01-15 10:30:45Z",
            "2024-01-15 10:30:45 +0000",
            "Mon, 15 Jan 2024 10:30:45 +0000",
            "2024-01-15T10:30:45+0000",
            "2024-01-15T10:30:45+00:00",
        ] {
            let ft =
                parse_date_string(input).unwrap_or_else(|e| panic!("'{input}' should parse: {e}"));
            assert_eq!(ft.unix_seconds(), EPOCH, "input: {input}");
        }

        // A non-zero offset shifts the instant, in every offset spelling.
        for input in [
            "2024-01-15 10:30:45 +0900",
            "2024-01-15T10:30:45+0900",
            "2024-01-15T10:30:45+09:00",
            "2024-01-15T10:30:45 +0900",
        ] {
            let ft =
                parse_date_string(input).unwrap_or_else(|e| panic!("'{input}' should parse: {e}"));
            assert_eq!(ft.unix_seconds(), EPOCH - 9 * 3600, "input: {input}");
        }
    }

    #[test]
    fn date_string_offset_forms_accept_fractions() {
        for input in ["2024-01-15T10:30:45.5+0000", "2024-01-15T10:30:45,5+0000"] {
            let ft =
                parse_date_string(input).unwrap_or_else(|e| panic!("'{input}' should parse: {e}"));
            assert_eq!(ft.unix_seconds(), 1_705_314_645, "input: {input}");
            assert_eq!(ft.nanoseconds(), 500_000_000, "input: {input}");
        }

        let ft = parse_date_string("2024-01-15T10:30:45.25+0900").unwrap();
        assert_eq!(ft.unix_seconds(), 1_705_314_645 - 9 * 3600);
        assert_eq!(ft.nanoseconds(), 250_000_000);
    }

    #[test]
    fn date_string_carries_fractional_seconds_into_nanos() {
        for input in ["2024-01-15T10:30:45.5Z", "2024-01-15T10:30:45,5Z"] {
            let ft =
                parse_date_string(input).unwrap_or_else(|e| panic!("'{input}' should parse: {e}"));
            assert_eq!(ft.unix_seconds(), 1_705_314_645, "input: {input}");
            assert_eq!(ft.nanoseconds(), 500_000_000, "input: {input}");
        }
    }

    #[test]
    fn date_string_rfc2822_comma_form_still_parses() {
        // chrono's whitespace item matches zero spaces, so this form parses;
        // the comma-fraction rule requires a digit on both sides ("n,1" here
        // has a letter before the comma) and must leave it alone.
        let ft = parse_date_string("Mon,15 Jan 2024 10:30:45 +0000").unwrap();
        assert_eq!(ft.unix_seconds(), 1_705_314_645);
    }

    #[test]
    fn date_string_rejects_malformed_posix_edges_without_panicking() {
        // Boundary shapes for the Z-strip and comma-fraction preprocessing:
        // trailing comma, comma against a multibyte char, bare or
        // multibyte-preceded Z.
        for input in [
            "2024-01-15 10:30:45,",
            ",5",
            "Z",
            "\u{20AC}Z",
            "2024-01-15T10:30:45,\u{20AC}",
        ] {
            assert!(parse_date_string(input).is_err(), "input: {input}");
        }
    }

    #[test]
    fn date_string_now_returns_a_time_near_now() {
        let before = FileTime::now();
        let ft = parse_date_string("now").unwrap();
        let after = FileTime::now();
        assert!(before <= ft && ft <= after);
    }

    #[test]
    fn date_string_rejects_garbage() {
        assert!(parse_date_string("not a date").is_err());
        assert!(parse_date_string("").is_err());
    }
}
