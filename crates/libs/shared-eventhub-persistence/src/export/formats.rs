//! Writing one message to a txt, csv, or json file, either alone or appended
//! to a condensed file. Every append leaves the file complete for its format.

use crate::export::ExportFormat;
use crate::export::render::{
    CONTENT_HEADER, ContentRow, METADATA_HEADER, MetadataRow, format_full_message,
};
use anyhow::Context;
use serde::Serialize;
use shared_eventhub_core::models::CapturedMessage;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// How a message is laid out in its file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout<'a> {
    pub(crate) format: ExportFormat,
    pub(crate) include_metadata: bool,
    pub(crate) condense: bool,
    pub(crate) entity_path: &'a str,
}

/// Writes `message` to `path` (creating missing folders): appended when the
/// layout is condensed and the file exists, otherwise replacing the file.
pub(crate) fn write_message(
    path: &Path,
    message: &CapturedMessage,
    layout: Layout<'_>,
) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create the folder '{}'", parent.display()))?;
    }
    let append = layout.condense && path.exists();

    match layout.format {
        ExportFormat::Txt => write_txt(path, message, layout, append),
        ExportFormat::Csv => write_csv(path, message, layout, append),
        ExportFormat::Json if layout.include_metadata => write_json(
            path,
            &MetadataRow::new(message, layout.entity_path),
            layout,
            append,
        ),
        ExportFormat::Json => write_json(
            path,
            &ContentRow {
                message_content: &message.body,
            },
            layout,
            append,
        ),
    }
    .with_context(|| format!("Failed to export to '{}'", path.display()))
}

fn write_txt(
    path: &Path,
    message: &CapturedMessage,
    layout: Layout<'_>,
    append: bool,
) -> anyhow::Result<()> {
    let content = if layout.include_metadata {
        format_full_message(message)
    } else {
        message.body.clone()
    };
    if !append {
        return std::fs::write(path, content).context("Failed to write the file");
    }

    let separator = if layout.include_metadata {
        "\n\n"
    } else {
        "\n"
    };
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .context("Failed to open the file for appending")?;
    file.write_all(format!("{separator}{content}").as_bytes())
        .context("Failed to append to the file")
}

fn write_csv(
    path: &Path,
    message: &CapturedMessage,
    layout: Layout<'_>,
    append: bool,
) -> anyhow::Result<()> {
    let header: &[&str] = if layout.include_metadata {
        &METADATA_HEADER
    } else {
        &CONTENT_HEADER
    };
    let append = append && !is_empty(path)?;
    if append {
        check_csv_header(path, header)?;
    }

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
        .context("Failed to open the file")?;
    let mut writer = csv::Writer::from_writer(file);
    if !append {
        writer
            .write_record(header)
            .context("Failed to write the CSV header")?;
    }
    if layout.include_metadata {
        let row = MetadataRow::new(message, layout.entity_path);
        let properties = row.properties_cell()?;
        writer
            .write_record(row.fields(&properties))
            .context("Failed to write the CSV record")?;
    } else {
        writer
            .write_record([message.body.as_str()])
            .context("Failed to write the CSV record")?;
    }

    writer.flush().context("Failed to flush the CSV file")
}

/// Refuses to append to a csv file whose header is not this layout's, so
/// rows with different columns never mix in one file.
fn check_csv_header(path: &Path, header: &[&str]) -> anyhow::Result<()> {
    let mut first_line = String::new();
    BufReader::new(File::open(path).context("Failed to open the file")?)
        .read_line(&mut first_line)
        .context("Failed to read the CSV header")?;
    let expected = header.join(",");

    if first_line.trim_end_matches(['\r', '\n']) != expected {
        anyhow::bail!(
            "The file has a different CSV header than this export writes ('{expected}'); export \
             to another folder or move the file"
        );
    }
    Ok(())
}

fn is_empty(path: &Path) -> anyhow::Result<bool> {
    Ok(std::fs::metadata(path)
        .context("Failed to read the file's metadata")?
        .len()
        == 0)
}

/// Writes one json row: a pretty object for an individual file, or an element
/// of a pretty array for a condensed one. The condensed file always equals
/// `serde_json::to_string_pretty` of the whole array.
fn write_json(
    path: &Path,
    row: &impl Serialize,
    layout: Layout<'_>,
    append: bool,
) -> anyhow::Result<()> {
    if !layout.condense {
        let object =
            serde_json::to_string_pretty(row).context("Failed to serialize the message")?;
        return std::fs::write(path, object).context("Failed to write the file");
    }

    let array = serde_json::to_string_pretty(&[row]).context("Failed to serialize the message")?;
    if !append {
        return std::fs::write(path, array).context("Failed to write the file");
    }
    let element = array
        .strip_prefix("[\n")
        .and_then(|rest| rest.strip_suffix("\n]"))
        .context("The serialized message is not a one-element array")?;
    append_array_element(path, element)
}

/// Appends an already indented `element` to the json array in `path`, by
/// rewriting only the array's closing bracket. A file that does not end in an
/// array is a failure and stays as it was; an empty or blank file starts a new
/// array.
fn append_array_element(path: &Path, element: &str) -> anyhow::Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .context("Failed to open the file")?;
    let length = file
        .metadata()
        .context("Failed to read the file's metadata")?
        .len();

    let Some((closing, last)) = last_non_whitespace(&mut file, length)? else {
        file.set_len(0).context("Failed to reset the blank file")?;
        file.seek(SeekFrom::Start(0))
            .context("Failed to seek to the start")?;
        return file
            .write_all(format!("[\n{element}\n]").as_bytes())
            .context("Failed to write the file");
    };
    if last != b']' {
        anyhow::bail!("The file does not end in a JSON array, so it was left unchanged");
    }
    let (keep, separator) = match last_non_whitespace(&mut file, closing)? {
        Some((position, b'[')) => (position + 1, "\n"),
        Some((position, _)) => (position + 1, ",\n"),
        None => anyhow::bail!("The file does not hold a JSON array, so it was left unchanged"),
    };

    file.set_len(keep)
        .context("Failed to truncate the array's end")?;
    file.seek(SeekFrom::End(0))
        .context("Failed to seek to the end")?;
    file.write_all(format!("{separator}{element}\n]").as_bytes())
        .context("Failed to append to the file")
}

/// The position and value of the last byte before `end` that is not ASCII
/// whitespace, read backwards in chunks.
fn last_non_whitespace(file: &mut File, end: u64) -> anyhow::Result<Option<(u64, u8)>> {
    const CHUNK: u64 = 4096;
    let mut buffer = [0_u8; CHUNK as usize];
    let mut end = end;

    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let length = usize::try_from(end - start).context("A chunk length overflowed")?;
        let chunk = &mut buffer[..length];
        file.seek(SeekFrom::Start(start))
            .context("Failed to seek in the file")?;
        file.read_exact(chunk).context("Failed to read the file")?;
        if let Some(index) = chunk.iter().rposition(|byte| !byte.is_ascii_whitespace()) {
            return Ok(Some((start + index as u64, chunk[index])));
        }
        end = start;
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema::test_messages::message;
    use rstest::rstest;
    use std::collections::BTreeMap;

    fn layout(format: ExportFormat, include_metadata: bool, condense: bool) -> Layout<'static> {
        Layout {
            format,
            include_metadata,
            condense,
            entity_path: "orders",
        }
    }

    fn write_all(path: &Path, messages: &[CapturedMessage], layout: Layout<'_>) {
        for message in messages {
            write_message(path, message, layout).unwrap();
        }
    }

    fn two_messages() -> Vec<CapturedMessage> {
        vec![
            message("0", 1, "first, \"quoted\""),
            message("1", 2, "second"),
        ]
    }

    #[test]
    fn txt_with_metadata_is_the_details_block() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("one.txt");
        let source = message("0", 1, "body");

        write_message(&path, &source, layout(ExportFormat::Txt, true, false)).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format_full_message(&source)
        );
    }

    #[rstest]
    #[case::metadata(true, "\n\n")]
    #[case::content_only(false, "\n")]
    fn condensed_txt_appends_with_a_separator(
        #[case] include_metadata: bool,
        #[case] separator: &str,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("month").join("messages.txt");
        let messages = two_messages();

        write_all(
            &path,
            &messages,
            layout(ExportFormat::Txt, include_metadata, true),
        );

        let render = |message: &CapturedMessage| {
            if include_metadata {
                format_full_message(message)
            } else {
                message.body.clone()
            }
        };
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!(
                "{}{separator}{}",
                render(&messages[0]),
                render(&messages[1])
            )
        );
    }

    #[test]
    fn individual_txt_replaces_an_existing_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("one.txt");
        std::fs::write(&path, "old content").unwrap();

        write_message(
            &path,
            &message("0", 1, "new"),
            layout(ExportFormat::Txt, false, false),
        )
        .unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn condensed_csv_with_metadata_matches_the_golden_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.csv");

        write_all(
            &path,
            &two_messages(),
            layout(ExportFormat::Csv, true, true),
        );

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "entity_path,partition_id,event_id,timestamp,content_type,correlation_id,properties,\
             message_content\n\
             orders,0,1,2026-07-02T12:30:01.000000000Z,,,{},\"first, \"\"quoted\"\"\"\n\
             orders,1,2,2026-07-02T12:30:02.000000000Z,,,{},second\n"
        );
    }

    fn with_event_metadata(mut message: CapturedMessage) -> CapturedMessage {
        message.content_type = Some("application/json".to_owned());
        message.correlation_id = Some("run-17".to_owned());
        message.properties = BTreeMap::from([
            ("source".to_owned(), "checkout".to_owned()),
            (
                "note".to_owned(),
                "a comma, a \"quote\"\nand a second line".to_owned(),
            ),
        ]);
        message
    }

    #[test]
    fn csv_with_metadata_round_trips_the_event_metadata_through_a_csv_parser() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.csv");
        let source = with_event_metadata(message("0", 1, "body"));

        write_message(&path, &source, layout(ExportFormat::Csv, true, true)).unwrap();

        let mut reader = csv::Reader::from_path(&path).unwrap();
        let header = reader.headers().unwrap().clone();
        let record = reader.records().next().unwrap().unwrap();
        let cell = |name: &str| {
            let index = header.iter().position(|column| column == name).unwrap();
            record.get(index).unwrap().to_owned()
        };
        let properties: BTreeMap<String, String> =
            serde_json::from_str(&cell("properties")).unwrap();
        assert_eq!(header.iter().collect::<Vec<_>>(), METADATA_HEADER);
        assert_eq!(cell("content_type"), "application/json");
        assert_eq!(cell("correlation_id"), "run-17");
        assert_eq!(properties, source.properties);
        assert_eq!(cell("message_content"), "body");
    }

    #[test]
    fn json_with_metadata_round_trips_the_event_metadata_through_a_json_parser() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("one.json");
        let source = with_event_metadata(message("0", 1, "body"));

        write_message(&path, &source, layout(ExportFormat::Json, true, false)).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let properties: BTreeMap<String, String> =
            serde_json::from_value(parsed["properties"].clone()).unwrap();
        assert_eq!(parsed["content_type"], "application/json");
        assert_eq!(parsed["correlation_id"], "run-17");
        assert_eq!(properties, source.properties);
        assert!(
            text.find("\"properties\"").unwrap() < text.find("\"message_content\"").unwrap(),
            "{text}"
        );
    }

    #[test]
    fn a_condensed_csv_with_the_five_column_header_is_refused_and_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.csv");
        std::fs::write(
            &path,
            "entity_path,partition_id,event_id,timestamp,message_content\n\
             orders,0,1,2026-07-02T12:30:01.000000000Z,first\n",
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();

        let error = write_message(
            &path,
            &message("0", 2, "y"),
            layout(ExportFormat::Csv, true, true),
        )
        .unwrap_err();

        assert!(
            format!("{error:#}").contains("different CSV header"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[rstest]
    #[case::csv(ExportFormat::Csv, "message_content\nbody\n")]
    #[case::json(ExportFormat::Json, "{\n  \"message_content\": \"body\"\n}")]
    #[case::txt(ExportFormat::Txt, "body")]
    fn an_export_without_metadata_holds_only_the_body(
        #[case] format: ExportFormat,
        #[case] expected: &str,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("one");
        let source = with_event_metadata(message("0", 1, "body"));

        write_message(&path, &source, layout(format, false, false)).unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
    }

    #[test]
    fn content_only_csv_has_one_column() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("one.csv");

        write_message(
            &path,
            &message("0", 1, "only"),
            layout(ExportFormat::Csv, false, false),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "message_content\nonly\n"
        );
    }

    #[test]
    fn a_condensed_csv_with_the_other_layout_is_refused_and_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.csv");
        write_message(
            &path,
            &message("0", 1, "x"),
            layout(ExportFormat::Csv, false, true),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();

        let error = write_message(
            &path,
            &message("0", 2, "y"),
            layout(ExportFormat::Csv, true, true),
        )
        .unwrap_err();

        assert!(
            format!("{error:#}").contains("different CSV header"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn individual_json_with_metadata_matches_the_golden_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("one.json");

        write_message(
            &path,
            &message("0", 7, "body"),
            layout(ExportFormat::Json, true, false),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"entity_path\": \"orders\",\n  \"partition_id\": \"0\",\n  \"event_id\": \"7\",\n  \
             \"timestamp\": \"2026-07-02T12:30:07.000000000Z\",\n  \"content_type\": null,\n  \
             \"correlation_id\": null,\n  \"properties\": {},\n  \"message_content\": \"body\"\n}"
        );
    }

    #[rstest]
    #[case::one(1)]
    #[case::two(2)]
    #[case::three(3)]
    fn condensed_json_equals_the_pretty_array(#[case] count: i64) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.json");
        let messages: Vec<CapturedMessage> = (1..=count)
            .map(|sequence| message("0", sequence, "x"))
            .collect();

        write_all(&path, &messages, layout(ExportFormat::Json, true, true));

        let rows: Vec<MetadataRow<'_>> = messages
            .iter()
            .map(|message| MetadataRow::new(message, "orders"))
            .collect();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            serde_json::to_string_pretty(&rows).unwrap()
        );
    }

    #[rstest]
    #[case::empty_array("[]")]
    #[case::trailing_newline("[\n  {\n    \"message_content\": \"a\"\n  }\n]\n")]
    #[case::blank("  \n")]
    fn condensed_json_appends_to_odd_but_valid_files(#[case] existing: &str) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.json");
        std::fs::write(&path, existing).unwrap();

        write_message(
            &path,
            &message("0", 1, "b"),
            layout(ExportFormat::Json, false, true),
        )
        .unwrap();

        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let contents: Vec<&str> = parsed
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["message_content"].as_str().unwrap())
            .collect();
        assert_eq!(contents.last(), Some(&"b"));
    }

    #[test]
    fn appending_to_an_empty_json_array_matches_the_pretty_array() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.json");
        std::fs::write(&path, "[]").unwrap();

        write_message(
            &path,
            &message("0", 1, "b"),
            layout(ExportFormat::Json, false, true),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            serde_json::to_string_pretty(&[ContentRow {
                message_content: "b"
            }])
            .unwrap()
        );
    }

    #[rstest]
    #[case::not_an_array("{\"a\": 1}")]
    #[case::only_a_bracket("]")]
    fn a_condensed_json_file_that_is_not_an_array_is_refused_and_left_alone(
        #[case] existing: &str,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.json");
        std::fs::write(&path, existing).unwrap();

        let error = write_message(
            &path,
            &message("0", 1, "b"),
            layout(ExportFormat::Json, false, true),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("left unchanged"), "{error:#}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);
    }

    #[test]
    fn a_long_whitespace_tail_is_scanned_across_chunks() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("messages.json");
        std::fs::write(&path, format!("[]{}", " ".repeat(10_000))).unwrap();

        write_message(
            &path,
            &message("0", 1, "b"),
            layout(ExportFormat::Json, false, true),
        )
        .unwrap();

        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_destination_that_cannot_be_written_names_the_path() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("blocker");
        std::fs::write(&blocker, "a file, not a folder").unwrap();
        let path = blocker.join("one.txt");

        let error = write_message(
            &path,
            &message("0", 1, "x"),
            layout(ExportFormat::Txt, true, false),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("blocker"), "{error:#}");
    }
}
