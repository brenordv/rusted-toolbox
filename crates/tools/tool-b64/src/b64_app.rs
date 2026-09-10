use crate::models::{B64Config, B64Mode, InputSource, OutputTarget};
use base64::engine::general_purpose::STANDARD;
use base64::write::EncoderWriter;
use base64::{DecodeSliceError, Engine};
use common_utils::constants::SIZE_64KB;
use std::fs::File;
use std::io::{self, BufWriter, Cursor, Read, Write};
use std::num::NonZeroUsize;
use std::path::Path;

/// Runs the Base64 tool with the provided configuration.
pub fn run(config: &B64Config) -> Result<(), AppError> {
    match config.mode {
        B64Mode::Encode => encode(config),
        B64Mode::Decode => decode(config),
    }
}

fn encode(config: &B64Config) -> Result<(), AppError> {
    let mut reader = open_reader(&config.input)?;
    let writer = open_writer(&config.output)?;

    let mut writer = BufWriter::with_capacity(SIZE_64KB, writer);
    let mut wrap_writer = WrapWriter::new(&mut writer, config.wrap_columns);
    let mut encoder = EncoderWriter::new(&mut wrap_writer, &STANDARD);

    for_each_chunk(&mut reader, &config.input, |chunk| {
        encoder
            .write_all(chunk)
            .map_err(|err| map_write_error(&config.output, err))
    })?;

    encoder
        .finish()
        .map_err(|err| map_write_error(&config.output, err))?;
    drop(encoder);

    wrap_writer
        .finish()
        .map_err(|err| map_write_error(&config.output, err))?;

    writer
        .flush()
        .map_err(|err| map_write_error(&config.output, err))?;

    Ok(())
}

fn decode(config: &B64Config) -> Result<(), AppError> {
    let mut reader = open_reader(&config.input)?;
    let writer = open_writer(&config.output)?;

    let mut writer = BufWriter::with_capacity(SIZE_64KB, writer);

    let mut pending: Vec<u8> = Vec::with_capacity(SIZE_64KB);
    let mut decoded_buffer: Vec<u8> = Vec::with_capacity(SIZE_64KB);
    let mut padding_seen = false;

    let keep: fn(u8) -> bool = if config.ignore_garbage {
        is_base64_byte_or_padding
    } else {
        is_not_line_break
    };

    for_each_chunk(&mut reader, &config.input, |chunk| {
        pending.extend(chunk.iter().copied().filter(|byte| keep(*byte)));

        if padding_seen && !pending.is_empty() {
            return Err(AppError::trailing_data_after_padding(&config.input));
        }

        let aligned = pending.len() / 4 * 4;
        if aligned == 0 {
            return Ok(());
        }

        decode_block(
            &pending[..aligned],
            &mut decoded_buffer,
            &mut writer,
            &config.input,
            &config.output,
        )?;

        if pending[aligned - 1] == b'=' {
            padding_seen = true;
        }

        pending.drain(..aligned);
        Ok(())
    })?;

    if padding_seen && !pending.is_empty() {
        return Err(AppError::trailing_data_after_padding(&config.input));
    }

    // The chunk loop decodes every complete quad, so any bytes left here form an
    // incomplete trailing quad (1-3 bytes), never a decodable block.
    if !pending.is_empty() {
        return Err(AppError::invalid_base64(
            &config.input,
            "invalid Base64 length",
        ));
    }

    writer
        .flush()
        .map_err(|err| map_write_error(&config.output, err))?;

    Ok(())
}

/// Reads `reader` in 64 KB chunks, invoking `f` on each non-empty chunk until EOF.
fn for_each_chunk(
    reader: &mut impl Read,
    source: &InputSource,
    mut f: impl FnMut(&[u8]) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let mut buffer = vec![0u8; SIZE_64KB];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| AppError::read_error(source, err))?;

        if read == 0 {
            break;
        }

        f(&buffer[..read])?;
    }

    Ok(())
}

/// Decodes one quad-aligned Base64 block and writes the resulting bytes.
fn decode_block(
    block: &[u8],
    decoded_buffer: &mut Vec<u8>,
    writer: &mut impl Write,
    source: &InputSource,
    output: &OutputTarget,
) -> Result<(), AppError> {
    let output_len = block.len() / 4 * 3;
    if decoded_buffer.len() < output_len {
        decoded_buffer.resize(output_len, 0);
    }

    let decoded_size = STANDARD
        .decode_slice(block, &mut decoded_buffer[..output_len])
        .map_err(|err| AppError::decode_error(source, err))?;

    writer
        .write_all(&decoded_buffer[..decoded_size])
        .map_err(|err| map_write_error(output, err))?;

    Ok(())
}

fn open_reader(source: &InputSource) -> Result<Box<dyn Read + '_>, AppError> {
    match source {
        InputSource::Stdin => Ok(Box::new(io::stdin().lock())),
        InputSource::File(path) => {
            let file = File::open(path).map_err(|err| AppError::cannot_open(path, err))?;
            Ok(Box::new(file))
        }
        InputSource::Text(text) => Ok(Box::new(Cursor::new(text.as_bytes()))),
    }
}

fn open_writer(target: &OutputTarget) -> Result<Box<dyn Write>, AppError> {
    match target {
        OutputTarget::Stdout => Ok(Box::new(io::stdout().lock())),
        OutputTarget::File(path) => {
            let file = File::create(path).map_err(|err| AppError::cannot_create(path, err))?;
            Ok(Box::new(file))
        }
    }
}

fn is_base64_byte_or_padding(byte: u8) -> bool {
    matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'=')
}

fn is_not_line_break(byte: u8) -> bool {
    byte != b'\r' && byte != b'\n'
}

fn map_write_error(target: &OutputTarget, err: io::Error) -> AppError {
    if err.kind() == io::ErrorKind::BrokenPipe {
        return AppError::broken_pipe();
    }

    match target {
        OutputTarget::Stdout => AppError::write_stdout(err),
        OutputTarget::File(path) => AppError::write_file(path, err),
    }
}

/// Wrap writer that inserts line feeds after a configured column width.
struct WrapWriter<'a, W: Write> {
    inner: &'a mut W,
    wrap_at: Option<NonZeroUsize>,
    column: usize,
}

impl<'a, W: Write> WrapWriter<'a, W> {
    fn new(inner: &'a mut W, wrap_at: Option<NonZeroUsize>) -> Self {
        Self {
            inner,
            wrap_at,
            column: 0,
        }
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.wrap_at.is_some() && self.column > 0 {
            self.inner.write_all(b"\n")?;
            self.column = 0;
        }
        self.inner.flush()
    }
}

impl<'a, W: Write> Write for WrapWriter<'a, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(limit) = self.wrap_at {
            let limit = limit.get();
            let mut written = 0;
            while written < buf.len() {
                let remaining_line = limit - self.column;
                let take = remaining_line.min(buf.len() - written);
                self.inner.write_all(&buf[written..written + take])?;
                self.column += take;
                written += take;

                if self.column == limit {
                    self.inner.write_all(b"\n")?;
                    self.column = 0;
                }
            }
            Ok(buf.len())
        } else {
            self.inner.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Error type carrying message and exit code.
#[derive(Debug)]
pub struct AppError {
    pub message: String,
    pub exit_code: i32,
}

impl AppError {
    fn new<S: Into<String>>(message: S, exit_code: i32) -> Self {
        Self {
            message: message.into(),
            exit_code,
        }
    }

    fn decode_error(source: &InputSource, err: DecodeSliceError) -> Self {
        Self::new(
            format!("decode error: {} (input: {})", err, input_label(source)),
            2,
        )
    }

    fn invalid_base64(source: &InputSource, reason: &str) -> Self {
        Self::new(
            format!("decode error: {} (input: {})", reason, input_label(source)),
            2,
        )
    }

    fn trailing_data_after_padding(source: &InputSource) -> Self {
        Self::new(
            format!(
                "decode error: trailing data after padding; input may be concatenated Base64 streams (input: {})",
                input_label(source)
            ),
            2,
        )
    }

    fn cannot_open(path: &Path, err: io::Error) -> Self {
        Self::new(
            format!("cannot open '{}': {}", path.to_string_lossy(), err),
            1,
        )
    }

    fn cannot_create(path: &Path, err: io::Error) -> Self {
        Self::new(
            format!(
                "cannot open output file '{}': {}",
                path.to_string_lossy(),
                err
            ),
            1,
        )
    }

    fn read_error(source: &InputSource, err: io::Error) -> Self {
        match source {
            InputSource::Stdin => Self::new(format!("error reading from stdin: {}", err), 1),
            InputSource::File(path) => Self::new(
                format!("error reading from '{}': {}", path.to_string_lossy(), err),
                1,
            ),
            InputSource::Text(_) => {
                Self::new(format!("error reading from inline text input: {}", err), 1)
            }
        }
    }

    fn write_stdout(err: io::Error) -> Self {
        Self::new(format!("error writing to stdout: {}", err), 1)
    }

    fn write_file(path: &Path, err: io::Error) -> Self {
        Self::new(
            format!("error writing to '{}': {}", path.to_string_lossy(), err),
            1,
        )
    }

    fn broken_pipe() -> Self {
        Self::new(String::new(), 0)
    }
}

fn input_label(source: &InputSource) -> String {
    match source {
        InputSource::Stdin => "stdin".to_string(),
        InputSource::File(path) => format!("'{}'", path.to_string_lossy()),
        InputSource::Text(_) => "inline text input".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn config(mode: B64Mode, input: InputSource, output: &Path) -> B64Config {
        B64Config {
            mode,
            wrap_columns: NonZeroUsize::new(76),
            ignore_garbage: false,
            input,
            output: OutputTarget::File(output.to_path_buf()),
        }
    }

    #[test]
    fn encode_default_wrap_adds_newline() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let output_path = dir.path().join("output.b64");
        fs::write(&input_path, b"hello world").unwrap();

        let cfg = config(B64Mode::Encode, InputSource::File(input_path), &output_path);
        run(&cfg).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=\n");
    }

    #[test]
    fn encode_without_wrap_disables_newline() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let output_path = dir.path().join("output.b64");
        fs::write(&input_path, b"hello world").unwrap();

        let cfg = B64Config {
            wrap_columns: NonZeroUsize::new(0),
            ..config(B64Mode::Encode, InputSource::File(input_path), &output_path)
        };
        run(&cfg).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn encode_inline_text_without_wrap() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.b64");

        let cfg = B64Config {
            wrap_columns: NonZeroUsize::new(0),
            ..config(
                B64Mode::Encode,
                InputSource::Text("hello world".to_string()),
                &output_path,
            )
        };
        run(&cfg).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn decode_strict_accepts_wrapped_input() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.b64");
        let output_path = dir.path().join("output.bin");
        fs::write(&input_path, b"aGVsbG8gd29ybGQ=\n").unwrap();

        let cfg = config(B64Mode::Decode, InputSource::File(input_path), &output_path);
        run(&cfg).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_inline_text_strict() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let cfg = config(
            B64Mode::Decode,
            InputSource::Text("aGVsbG8gd29ybGQ=".to_string()),
            &output_path,
        );
        run(&cfg).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_inline_text_ignore_garbage() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let cfg = B64Config {
            ignore_garbage: true,
            ..config(
                B64Mode::Decode,
                InputSource::Text(" aGVs\nbG8gd29ybGQ=\t".to_string()),
                &output_path,
            )
        };
        run(&cfg).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_ignore_garbage_filters_noise() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.b64");
        let output_path = dir.path().join("output.bin");
        fs::write(&input_path, b"\t aGVsbG8gd29y bGQ=\n$%^\n").unwrap();

        let cfg = B64Config {
            ignore_garbage: true,
            ..config(B64Mode::Decode, InputSource::File(input_path), &output_path)
        };
        run(&cfg).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_invalid_input_returns_data_error() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.b64");
        let output_path = dir.path().join("output.bin");
        fs::write(&input_path, b"aGVsbG8gd29ybGQ=!").unwrap();

        let cfg = config(B64Mode::Decode, InputSource::File(input_path), &output_path);

        let err = run(&cfg).err().unwrap();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("decode error"));
    }

    #[test]
    fn encode_large_input_wraps_and_round_trips() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let encoded_path = dir.path().join("encoded.b64");
        let decoded_path = dir.path().join("roundtrip.bin");

        let original: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&input_path, &original).unwrap();

        let encode_cfg = config(
            B64Mode::Encode,
            InputSource::File(input_path),
            &encoded_path,
        );
        run(&encode_cfg).unwrap();

        let encoded = fs::read_to_string(&encoded_path).unwrap();
        assert!(encoded.ends_with('\n'));
        let lines: Vec<&str> = encoded.trim_end_matches('\n').split('\n').collect();
        let (last, rest) = lines.split_last().unwrap();
        assert!(rest.iter().all(|line| line.len() == 76));
        assert!(!last.is_empty() && last.len() <= 76);

        let decode_cfg = config(
            B64Mode::Decode,
            InputSource::File(encoded_path),
            &decoded_path,
        );
        run(&decode_cfg).unwrap();

        let decoded = fs::read(&decoded_path).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_wraps_without_trailing_blank_line_on_exact_boundary() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let output_path = dir.path().join("output.b64");
        fs::write(&input_path, vec![0u8; 570]).unwrap();

        let cfg = config(B64Mode::Encode, InputSource::File(input_path), &output_path);
        run(&cfg).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert!(encoded.ends_with('\n'));
        assert!(!encoded.ends_with("\n\n"));
        let lines: Vec<&str> = encoded.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 10);
        assert!(lines.iter().all(|line| line.len() == 76));
    }

    #[test]
    fn decode_rejects_data_after_padding_regardless_of_chunk_boundary() {
        let dir = tempdir().unwrap();

        let mut boundary_stream = "AAAA".repeat(16383);
        boundary_stream.push_str("AA==");
        boundary_stream.push_str("AAAAAAAA");
        let boundary_input = dir.path().join("boundary.b64");
        let boundary_output = dir.path().join("boundary.bin");
        fs::write(&boundary_input, boundary_stream.as_bytes()).unwrap();

        let boundary_cfg = config(
            B64Mode::Decode,
            InputSource::File(boundary_input),
            &boundary_output,
        );
        let boundary_err = run(&boundary_cfg).err().unwrap();
        assert_eq!(boundary_err.exit_code, 2);
        assert!(boundary_err.message.starts_with("decode error:"));
        assert!(!fs::read(&boundary_output).unwrap().is_empty());

        let small_input = dir.path().join("small.b64");
        let small_output = dir.path().join("small.bin");
        fs::write(&small_input, b"AAAAAA==AAAA").unwrap();

        let small_cfg = config(
            B64Mode::Decode,
            InputSource::File(small_input),
            &small_output,
        );
        let small_err = run(&small_cfg).err().unwrap();
        assert_eq!(small_err.exit_code, 2);
        assert!(small_err.message.starts_with("decode error:"));
        assert!(fs::read(&small_output).unwrap().is_empty());
    }

    #[test]
    fn decode_rejects_concatenated_padded_streams_in_single_chunk() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let cfg = config(
            B64Mode::Decode,
            InputSource::Text("SGVsbG8=SGVsbG8=".to_string()),
            &output_path,
        );

        let err = run(&cfg).err().unwrap();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.starts_with("decode error:"));
        assert!(fs::read(&output_path).unwrap().is_empty());
    }

    #[test]
    fn decode_incomplete_final_quad_reports_invalid_length() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let cfg = config(
            B64Mode::Decode,
            InputSource::Text("SGVsbG".to_string()),
            &output_path,
        );

        let err = run(&cfg).err().unwrap();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("invalid Base64 length"));
    }

    #[test]
    fn decode_missing_input_file_reports_cannot_open() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does_not_exist.b64");
        let output_path = dir.path().join("output.bin");

        let cfg = config(B64Mode::Decode, InputSource::File(missing), &output_path);

        let err = run(&cfg).err().unwrap();
        assert_eq!(err.exit_code, 1);
        assert!(err.message.contains("cannot open"));
    }

    #[test]
    fn wrap_writer_inserts_newline_at_each_wrap_boundary() {
        let mut out: Vec<u8> = Vec::new();
        let mut writer = WrapWriter::new(&mut out, NonZeroUsize::new(4));
        writer.write_all(b"abcdefghij").unwrap();
        writer.finish().unwrap();

        assert_eq!(out, b"abcd\nefgh\nij\n");
    }

    #[test]
    fn wrap_writer_defers_trailing_newline_until_finish() {
        let mut out: Vec<u8> = Vec::new();
        let mut writer = WrapWriter::new(&mut out, NonZeroUsize::new(4));
        writer.write_all(b"abcdef").unwrap();

        assert_eq!(out, b"abcd\nef");
    }

    #[test]
    fn wrap_writer_finish_adds_nothing_at_exact_boundary() {
        let mut out: Vec<u8> = Vec::new();
        let mut writer = WrapWriter::new(&mut out, NonZeroUsize::new(4));
        writer.write_all(b"abcdefgh").unwrap();
        writer.finish().unwrap();

        assert_eq!(out, b"abcd\nefgh\n");
    }

    #[test]
    fn wrap_writer_finish_emits_nothing_without_writes() {
        let mut out: Vec<u8> = Vec::new();
        let mut writer = WrapWriter::new(&mut out, NonZeroUsize::new(4));
        writer.finish().unwrap();

        assert!(out.is_empty());
    }

    #[test]
    fn wrap_writer_without_wrap_passes_bytes_through_unchanged() {
        let mut out: Vec<u8> = Vec::new();
        let mut writer = WrapWriter::new(&mut out, None);
        writer.write_all(b"abcdefghij").unwrap();
        writer.finish().unwrap();

        assert_eq!(out, b"abcdefghij");
    }

    #[test]
    fn encode_empty_input_with_wrap_produces_empty_output() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let output_path = dir.path().join("output.b64");
        fs::write(&input_path, b"").unwrap();

        let cfg = config(B64Mode::Encode, InputSource::File(input_path), &output_path);
        run(&cfg).unwrap();

        assert!(fs::read(&output_path).unwrap().is_empty());
    }

    #[test]
    fn decode_ignore_garbage_whitespace_only_input_yields_empty_output() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let cfg = B64Config {
            ignore_garbage: true,
            ..config(
                B64Mode::Decode,
                InputSource::Text(" \t\r\n ".to_string()),
                &output_path,
            )
        };
        run(&cfg).unwrap();

        assert!(fs::read(&output_path).unwrap().is_empty());
    }

    #[test]
    fn decode_ignore_garbage_padding_only_input_reports_invalid_length() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let cfg = B64Config {
            ignore_garbage: true,
            ..config(
                B64Mode::Decode,
                InputSource::Text(" == \n".to_string()),
                &output_path,
            )
        };

        let err = run(&cfg).err().unwrap();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("invalid Base64 length"));
        assert!(fs::read(&output_path).unwrap().is_empty());
    }
}
