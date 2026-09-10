pub enum RuntimeType {
    Host,
    Client,
}

/// A line delivered to the chat UI over the incoming-message channel.
pub enum UiMessage {
    /// Decrypted text received from the peer.
    Peer(String),
    /// Locally generated status line, rendered with a "[system]" prefix.
    System(String),
}
