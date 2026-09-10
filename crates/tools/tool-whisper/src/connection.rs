use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::TcpStream;
use tracing::debug;

/// Largest frame accepted from or written to the wire, in bytes.
///
/// The largest legitimate frame is the base64-encoded RSA-4096 public key exchanged during the
/// handshake (roughly 750 bytes); encrypted chat messages are smaller. Capping frames at 64 KiB
/// keeps a hostile or corrupted length header from driving a multi-gigabyte allocation.
pub const MAX_FRAME_LEN: usize = 64 * 1024;

/// Length-prefixed frame reader/writer over any bidirectional byte stream.
pub struct Connection<S: Read + Write> {
    pub connection: S,
    outgoing_count: usize,
    incoming_count: usize,
}

/// The TCP-backed connection used by the chat session.
pub type TcpConnection = Connection<TcpStream>;

impl<S: Read + Write> Connection<S> {
    pub fn new_from_connection(connection: S) -> Result<Self> {
        Ok(Self {
            connection,
            outgoing_count: 0,
            incoming_count: 0,
        })
    }

    /// Reads one length-prefixed frame. A zero-length header yields `Ok(None)`.
    ///
    /// # Errors
    /// Fails when the header or payload cannot be read, or when the header announces a frame
    /// larger than [`MAX_FRAME_LEN`] (rejected before any payload allocation).
    pub fn read_message(&mut self) -> Result<Option<String>> {
        self.incoming_count += 1;
        match self.read_message_header()? {
            None => Ok(None),
            Some(message_len) => {
                debug!(
                    frame = self.incoming_count,
                    bytes = message_len,
                    "Reading message"
                );
                let mut message_buf = vec![0u8; message_len];
                self.connection
                    .read_exact(&mut message_buf)
                    .with_context(|| {
                        format!(
                            "failed to read message {} of size {}",
                            self.incoming_count, message_len
                        )
                    })?;
                let message = String::from_utf8_lossy(&message_buf).to_string();
                debug!(bytes = message.len(), "Message read");
                Ok(Some(message))
            }
        }
    }

    /// Writes one length-prefixed frame. Empty messages are skipped.
    ///
    /// # Errors
    /// Fails when the payload exceeds [`MAX_FRAME_LEN`] or when the stream write fails.
    pub fn write_message(&mut self, message: &str) -> Result<()> {
        if message.is_empty() {
            debug!("Skipping empty message");
            return Ok(());
        }

        let message_bytes = message.as_bytes();
        if message_bytes.len() > MAX_FRAME_LEN {
            anyhow::bail!(
                "refusing to write a frame of {} bytes: the frame cap is {} bytes",
                message_bytes.len(),
                MAX_FRAME_LEN
            );
        }

        // The cap check above bounds the length far below u32::MAX, so this cannot fail.
        let message_len = u32::try_from(message_bytes.len())
            .context("frame length does not fit in the 4-byte header")?;

        self.outgoing_count += 1;

        debug!(
            frame = self.outgoing_count,
            bytes = message_len,
            "Sending message header"
        );
        self.connection.write_all(&message_len.to_be_bytes())?;

        debug!(
            frame = self.outgoing_count,
            bytes = message_len,
            "Sending message payload"
        );
        self.connection.write_all(message_bytes)?;

        Ok(())
    }

    fn read_message_header(&mut self) -> Result<Option<usize>> {
        debug!("Reading message header...");

        let mut len_buf = [0u8; 4];
        self.connection.read_exact(&mut len_buf).with_context(|| {
            format!(
                "failed to read the header for message {}",
                self.incoming_count
            )
        })?;

        let message_len = u32::from_be_bytes(len_buf) as usize;

        if message_len == 0 {
            return Ok(None);
        }

        if message_len > MAX_FRAME_LEN {
            anyhow::bail!(
                "rejecting a frame of {} bytes: the frame cap is {} bytes",
                message_len,
                MAX_FRAME_LEN
            );
        }

        Ok(Some(message_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    /// In-memory stream: reads come from `input`, writes accumulate in `output`.
    struct MemoryStream {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl MemoryStream {
        fn with_input(bytes: Vec<u8>) -> Self {
            Self {
                input: Cursor::new(bytes),
                output: Vec::new(),
            }
        }

        fn empty() -> Self {
            Self::with_input(Vec::new())
        }
    }

    impl Read for MemoryStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for MemoryStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.output.flush()
        }
    }

    fn read_back(wire_bytes: Vec<u8>) -> Connection<MemoryStream> {
        Connection::new_from_connection(MemoryStream::with_input(wire_bytes))
            .expect("connection over an in-memory stream")
    }

    #[test]
    fn write_then_read_round_trips_a_frame() {
        let mut writer = Connection::new_from_connection(MemoryStream::empty()).unwrap();
        writer.write_message("hello, frame").unwrap();

        let mut reader = read_back(writer.connection.output);

        assert_eq!(
            reader.read_message().unwrap().as_deref(),
            Some("hello, frame")
        );
    }

    #[test]
    fn frame_exactly_at_the_cap_round_trips() {
        let payload = "a".repeat(MAX_FRAME_LEN);
        let mut writer = Connection::new_from_connection(MemoryStream::empty()).unwrap();
        writer.write_message(&payload).unwrap();

        let mut reader = read_back(writer.connection.output);

        assert_eq!(
            reader.read_message().unwrap().as_deref(),
            Some(&payload[..])
        );
    }

    #[test]
    fn read_message_rejects_frame_over_cap_without_reading_payload() {
        let oversized = (MAX_FRAME_LEN as u32) + 1;
        let mut reader = read_back(oversized.to_be_bytes().to_vec());

        let err = reader.read_message().unwrap_err();
        let message = format!("{err:#}");

        assert!(message.contains(&oversized.to_string()));
        assert!(message.contains(&MAX_FRAME_LEN.to_string()));
    }

    #[test]
    fn zero_length_header_reads_as_none() {
        let mut reader = read_back(vec![0, 0, 0, 0]);

        assert!(reader.read_message().unwrap().is_none());
    }

    #[test]
    fn empty_message_writes_nothing() {
        let mut writer = Connection::new_from_connection(MemoryStream::empty()).unwrap();

        writer.write_message("").unwrap();

        assert!(writer.connection.output.is_empty());
    }

    #[test]
    fn write_message_rejects_payload_over_the_cap() {
        let payload = "a".repeat(MAX_FRAME_LEN + 1);
        let mut writer = Connection::new_from_connection(MemoryStream::empty()).unwrap();

        assert!(writer.write_message(&payload).is_err());
        assert!(writer.connection.output.is_empty());
    }

    #[test]
    fn read_from_exhausted_stream_surfaces_an_io_error() {
        let mut reader = read_back(Vec::new());

        let err = reader.read_message().unwrap_err();

        assert!(err.downcast_ref::<io::Error>().is_some());
    }
}
