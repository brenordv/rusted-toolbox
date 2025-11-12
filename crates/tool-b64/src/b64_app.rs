use crate::models::{B64Config, B64Mode, InputSource, OutputTarget};
use base64::engine::general_purpose::STANDARD;
use base64::{DecodeSliceError, Engine};
use shared::constants::general::SIZE_64KB;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Cursor, Read, Write};
use std::num::NonZeroUsize;
use std::path::Path;

/// Runs the Base64 tool with the provided configuration.
pub fn run(config: &B64Config) -> Result<(), AppError> {
    match config.mode {
        B64Mode::Encode => encode(config),
        B64Mode::Decode => decode(config),
    }
}

fn encoded_capacity(len: usize) -> usize {
    ((len + 2) / 3) * 4
}

fn encode(config: &B64Config) -> Result<(), AppError> {
    let reader = open_reader(&config.input)?;
    let writer = open_writer(&config.output)?;

    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut wrap_writer = WrapWriter::new(&mut writer, config.wrap_columns);

    let mut input_buffer = vec![0u8; SIZE_64KB];
    let mut pending = Vec::with_capacity(2);
    let mut encoded_buffer = Vec::with_capacity(encoded_capacity(SIZE_64KB));

    loop {
        let read = reader
            .read(&mut input_buffer)
            .map_err(|err| AppError::read_error(&config.input, err))?;

        if read == 0 {
            break;
        }

        pending.extend_from_slice(&input_buffer[..read]);

        let aligned = pending.len() / 3 * 3;
        if aligned > 0 {
            encoded_buffer.resize(encoded_capacity(aligned), 0);
            let encoded_len = STANDARD
                .encode_slice(&pending[..aligned], &mut encoded_buffer)
                .map_err(|_| AppError::encode_error())?;

            write_all(
                &mut wrap_writer,
                &encoded_buffer[..encoded_len],
                &config.output,
            )?;

            let remainder = pending[aligned..].to_vec();
            pending.clear();
            pending.extend_from_slice(&remainder);
        }
    }

    if !pending.is_empty() {
        encoded_buffer.resize(encoded_capacity(pending.len()), 0);
        let encoded_len = STANDARD
            .encode_slice(&pending, &mut encoded_buffer)
            .map_err(|_| AppError::encode_error())?;

        write_all(
            &mut wrap_writer,
            &encoded_buffer[..encoded_len],
            &config.output,
        )?;
    }

    wrap_writer
        .finish()
        .map_err(|err| map_write_error(&config.output, err))?;

    writer
        .flush()
        .map_err(|err| map_write_error(&config.output, err))?;

    Ok(())
}

fn decode(config: &B64Config) -> Result<(), AppError> {
    let reader = open_reader(&config.input)?;
    let writer = open_writer(&config.output)?;

    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

    let mut input_buffer = vec![0u8; SIZE_64KB];
    let mut pending: Vec<u8> = Vec::with_capacity(SIZE_64KB);
    let mut decoded_buffer: Vec<u8> = Vec::with_capacity(SIZE_64KB);

    loop {
        let read = reader
            .read(&mut input_buffer)
            .map_err(|err| AppError::read_error(&config.input, err))?;

        if read == 0 {
            break;
        }

        if config.ignore_garbage {
            pending.extend(
                input_buffer[..read]
                    .iter()
                    .copied()
                    .filter(|byte| is_base64_byte_or_padding(*byte)),
            );
        } else {
            pending.extend(
                input_buffer[..read]
                    .iter()
                    .copied()
                    .filter(|byte| *byte != b'\r' && *byte != b'\n'),
            );
        }

        let aligned = pending.len() / 4 * 4;
        if aligned == 0 {
            continue;
        }

        let output_len = (aligned / 4) * 3;
        if decoded_buffer.len() < output_len {
            decoded_buffer.resize(output_len, 0);
        }

        match STANDARD.decode_slice(&pending[..aligned], &mut decoded_buffer[..output_len]) {
            Ok(decoded_size) => {
                write_all(&mut writer, &decoded_buffer[..decoded_size], &config.output)?;
            }
            Err(err) => {
                return Err(AppError::decode_error(err));
            }
        }

        let remainder = pending[aligned..].to_vec();
        pending.clear();
        pending.extend_from_slice(&remainder);
    }

    if !pending.is_empty() {
        if pending.len() % 4 != 0 {
            return Err(AppError::invalid_base64(
                "decode error: invalid Base64 length",
            ));
        }

        let output_len = (pending.len() / 4) * 3;
        if decoded_buffer.len() < output_len {
            decoded_buffer.resize(output_len, 0);
        }

        match STANDARD.decode_slice(&pending, &mut decoded_buffer[..output_len]) {
            Ok(decoded_size) => {
                write_all(&mut writer, &decoded_buffer[..decoded_size], &config.output)?;
            }
            Err(err) => {
                return Err(AppError::decode_error(err));
            }
        }
    }

    writer
        .flush()
        .map_err(|err| map_write_error(&config.output, err))?;

    Ok(())
}

fn open_reader(source: &InputSource) -> Result<Box<dyn Read>, AppError> {
    match source.clone() {
        InputSource::Stdin => Ok(Box::new(io::stdin())),
        InputSource::File(path) => File::open(&path)
            .map(|file| Box::new(file) as Box<dyn Read>)
            .map_err(|err| AppError::cannot_open(&path, err)),
        InputSource::Text(text) => Ok(Box::new(Cursor::new(text.into_bytes()))),
    }
}

fn open_writer(target: &OutputTarget) -> Result<Box<dyn Write>, AppError> {
    match target {
        OutputTarget::Stdout => Ok(Box::new(io::stdout())),
        OutputTarget::File(path) => File::create(path)
            .map(|file| Box::new(file) as Box<dyn Write>)
            .map_err(|err| AppError::cannot_create(path, err)),
    }
}

fn write_all<W: Write>(writer: &mut W, data: &[u8], target: &OutputTarget) -> Result<(), AppError> {
    writer
        .write_all(data)
        .map_err(|err| map_write_error(target, err))?;
    Ok(())
}

fn is_base64_byte_or_padding(byte: u8) -> bool {
    matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'=')
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

    fn encode_error() -> Self {
        Self::new("encode error: internal encoder failure", 1)
    }

    fn decode_error(err: DecodeSliceError) -> Self {
        Self::new(format!("decode error: {}", err), 2)
    }

    fn invalid_base64(message: &str) -> Self {
        Self::new(message.to_string(), 2)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::num::NonZeroUsize;
    use std::path::Path;
    use tempfile::tempdir;

    fn encode_config(input: &Path, output: &Path, wrap: Option<usize>) -> B64Config {
        B64Config {
            mode: B64Mode::Encode,
            wrap_columns: wrap.and_then(NonZeroUsize::new),
            ignore_garbage: false,
            input: InputSource::File(input.to_path_buf()),
            output: OutputTarget::File(output.to_path_buf()),
        }
    }

    fn decode_config(input: &Path, output: &Path, ignore_garbage: bool) -> B64Config {
        B64Config {
            mode: B64Mode::Decode,
            wrap_columns: NonZeroUsize::new(76),
            ignore_garbage,
            input: InputSource::File(input.to_path_buf()),
            output: OutputTarget::File(output.to_path_buf()),
        }
    }

    fn encode_text_config(text: &str, output: &Path, wrap: Option<usize>) -> B64Config {
        B64Config {
            mode: B64Mode::Encode,
            wrap_columns: wrap.and_then(NonZeroUsize::new),
            ignore_garbage: false,
            input: InputSource::Text(text.to_string()),
            output: OutputTarget::File(output.to_path_buf()),
        }
    }

    fn decode_text_config(text: &str, output: &Path, ignore_garbage: bool) -> B64Config {
        B64Config {
            mode: B64Mode::Decode,
            wrap_columns: NonZeroUsize::new(76),
            ignore_garbage,
            input: InputSource::Text(text.to_string()),
            output: OutputTarget::File(output.to_path_buf()),
        }
    }

    #[test]
    fn encode_default_wrap_adds_newline() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let output_path = dir.path().join("output.b64");

        fs::write(&input_path, b"hello world").unwrap();

        let config = encode_config(&input_path, &output_path, Some(76));

        run(&config).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=\n");
    }

    #[test]
    fn encode_without_wrap_disables_newline() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.bin");
        let output_path = dir.path().join("output.b64");

        fs::write(&input_path, b"hello world").unwrap();

        let config = encode_config(&input_path, &output_path, Some(0));

        run(&config).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn encode_inline_text_without_wrap() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.b64");

        let config = encode_text_config("hello world", &output_path, Some(0));

        run(&config).unwrap();

        let encoded = fs::read_to_string(&output_path).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn decode_strict_accepts_wrapped_input() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.b64");
        let output_path = dir.path().join("output.bin");

        fs::write(&input_path, b"aGVsbG8gd29ybGQ=\n").unwrap();

        let config = decode_config(&input_path, &output_path, false);

        run(&config).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_inline_text_strict() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let config = decode_text_config("aGVsbG8gd29ybGQ=", &output_path, false);

        run(&config).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_inline_text_ignore_garbage() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("output.bin");

        let config = decode_text_config(" aGVs\nbG8gd29ybGQ=\t", &output_path, true);

        run(&config).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_ignore_garbage_filters_noise() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.b64");
        let output_path = dir.path().join("output.bin");

        fs::write(&input_path, b"\t aGVsbG8gd29y bGQ=\n$%^\n").unwrap();

        let config = decode_config(&input_path, &output_path, true);

        run(&config).unwrap();

        let decoded = fs::read(&output_path).unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_invalid_input_returns_data_error() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.b64");
        let output_path = dir.path().join("output.bin");

        fs::write(&input_path, b"aGVsbG8gd29ybGQ=!").unwrap();

        let config = decode_config(&input_path, &output_path, false);

        let result = run(&config);
        assert!(result.is_err());

        let err = result.err().unwrap();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("decode error"));
    }
}
