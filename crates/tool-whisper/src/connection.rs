use anyhow::Result;
use std::io::{Read, Write};
use std::net::TcpStream;
use tracing::{debug, error};

pub struct Connection {
    pub connection: TcpStream,
    outgoing_count: usize,
    incoming_count: usize,
}

impl Connection {
    pub fn new_from_connection(connection: TcpStream) -> Result<Self> {
        Ok(Self {
            connection,
            outgoing_count: 0,
            incoming_count: 0,
        })
    }

    pub fn read_message(&mut self) -> Result<Option<String>> {
        self.incoming_count += 1;
        match self.read_message_header()? {
            None => Ok(None),
            Some(message_len) => {
                debug!(
                    "Reading message {} of {} bytes",
                    self.incoming_count, message_len
                );
                let mut message_buf = vec![0u8; message_len];
                match self.connection.read_exact(&mut message_buf) {
                    Ok(()) => {
                        let message = String::from_utf8_lossy(&message_buf).to_string();
                        debug!("Message read: {}", message);
                        Ok(Some(message))
                    }
                    Err(e) => {
                        let err_msg = format!(
                            "Error reading message {} of size {}. Error: {}",
                            self.incoming_count, message_len, e
                        );
                        error!(err_msg);
                        anyhow::bail!(err_msg);
                    }
                }
            }
        }
    }

    pub fn write_message(&mut self, message: &str) -> Result<()> {
        if message.is_empty() {
            debug!("Skipping empty message");
            return Ok(());
        }

        let message_bytes = message.as_bytes();
        let message_len = message_bytes.len() as u32;

        self.outgoing_count += 1;

        debug!(
            "Sending headers for message {}. Size: {}",
            self.outgoing_count, message_len
        );
        self.connection.write_all(&message_len.to_be_bytes())?;

        debug!(
            "Sending message {}. Size: {}",
            self.outgoing_count, message_len
        );
        self.connection.write_all(message_bytes)?;

        Ok(())
    }

    fn read_message_header(&mut self) -> Result<Option<usize>> {
        debug!("Reading message header...");

        // Read message length first (4 bytes). Theoretical limit of 4GB per message, but please don't do that.
        let mut len_buf = [0u8; 4];

        if let Err(e) = self.connection.read_exact(&mut len_buf) {
            let err_msg = format!(
                "Error reading the headers for message {}. Error: {}",
                self.incoming_count, e
            );
            error!(err_msg);
            anyhow::bail!(err_msg);
        }

        let message_len = u32::from_be_bytes(len_buf) as usize;

        if message_len == 0 {
            return Ok(None);
        }

        Ok(Some(message_len))
    }
}
