use anyhow::{Context, Result};
use arboard::Clipboard;

/// Copies `text` to the system clipboard.
///
/// # Errors
/// Returns an error when the system clipboard cannot be accessed (for example in
/// a headless environment) or cannot be written to.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("failed to access the system clipboard")?;
    clipboard
        .set_text(text)
        .context("failed to write text to the system clipboard")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // `copy_to_clipboard` is deliberately untested: it mutates the real system
    // clipboard and fails in headless environments, so it cannot run
    // deterministically across the CI matrix.
}
