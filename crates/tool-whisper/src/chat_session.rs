use crate::connection::Connection;
use crate::encrypt::encryption::Encryption;
use crate::encrypt::message_decrypter::MessageDecrypter;
use crate::encrypt::message_encrypter::MessageEncrypter;
use anyhow::Result;
use std::sync::mpsc::{Receiver, Sender};

pub struct ChatSession {
    role: String,
    connection: Connection,
    my_encryption: Encryption,
    peer_encryption: MessageEncrypter,
    tx_outgoing_messages: Sender<String>,
    tx_incoming_messages: Sender<String>,
    pub rx_incoming_messages: Receiver<String>,
    rx_outgoing_messages: Receiver<String>,
}

impl ChatSession {
    pub fn new(
        role: String,
        connection: Connection,
        my_encryption: Encryption,
        peer_encryption: MessageEncrypter,
        tx_outgoing_messages: Sender<String>,
        tx_incoming_messages: Sender<String>,
        rx_incoming_messages: Receiver<String>,
        rx_outgoing_messages: Receiver<String>,
    ) -> Self {
        Self {
            role,
            connection,
            my_encryption,
            peer_encryption,
            tx_outgoing_messages,
            tx_incoming_messages,
            rx_incoming_messages,
            rx_outgoing_messages,
        }
    }

    pub fn get_role_name(&self) -> String {
        self.role.clone()
    }

    pub fn get_message_encrypter(&self) -> MessageEncrypter {
        self.peer_encryption.clone()
    }

    pub fn get_message_decrypter(&self) -> &MessageDecrypter {
        self.my_encryption.get_decrypter()
    }

    pub fn take_outgoing_receiver(&mut self) -> Receiver<String> {
        // We need to replace the receiver with a dummy one since we're moving it out
        let (_, dummy_rx) = std::sync::mpsc::channel();
        std::mem::replace(&mut self.rx_outgoing_messages, dummy_rx)
    }

    pub fn get_incoming_transmitter(&self) -> Sender<String> {
        self.tx_incoming_messages.clone()
    }

    pub fn get_outgoing_transmitter(&self) -> Sender<String> {
        self.tx_outgoing_messages.clone()
    }

    pub fn take_incoming_receiver(&mut self) -> Receiver<String> {
        // We need to replace the receiver with a dummy one since we're moving it out
        let (_, dummy_rx) = std::sync::mpsc::channel();
        std::mem::replace(&mut self.rx_incoming_messages, dummy_rx)
    }

    pub fn split_connection(&mut self) -> Result<Connection> {
        let conn = self.connection.connection.try_clone()?;
        Connection::new_from_connection(conn)
    }
}
