use std::path::Path;

pub const MSG_NO_RECIPIENTS: &str =
    "no recipients specified; pass -r <age1...> or -R <recipients-file>";

pub fn invalid_recipient_literal(index: usize) -> String {
    format!(
        "recipient argument {} is not a valid age public key (expected an age1... value)",
        index + 1
    )
}

pub fn invalid_recipient_in_file(path: &Path, line: usize) -> String {
    format!(
        "{}: line {}: not a valid age public key (expected an age1... value)",
        path.display(),
        line
    )
}

pub fn recipient_file_unreadable(path: &Path) -> String {
    format!("could not read recipients file {}", path.display())
}

pub fn identity_exists(path: &Path) -> String {
    format!(
        "identity file {} already exists; pass --force to overwrite it",
        path.display()
    )
}

pub fn identity_file_unreadable(path: &Path) -> String {
    format!("could not read identity file {}", path.display())
}

/// Reports only the file and 1-based line number, never the line's content: a
/// malformed identity line could be a mistyped secret key.
pub fn identity_file_malformed(path: &Path, line: usize) -> String {
    format!(
        "{}: line {}: not a valid age identity",
        path.display(),
        line
    )
}
