//! The system's light or dark appearance, as the app reads it itself.
//!
//! On Windows the app reads it the way Microsoft documents for Win32 apps: `UISettings`'
//! foreground color, where a light foreground means dark mode. Rune follows the appearance only
//! through the system's theme change event, which does not reach it on Windows 10. Elsewhere
//! Rune's value stands.

use tracing::{debug, info, warn};

/// Whether a color reads as light: Microsoft's `IsColorLight`, a quick perceived-brightness
/// check (`5G + 2R + B > 8 * 128`).
pub fn is_light(r: u8, g: u8, b: u8) -> bool {
    5 * u32::from(g) + 2 * u32::from(r) + u32::from(b) > 8 * 128
}

/// Turns each read of the appearance into a change to apply, and logs failed reads once per
/// run of failures.
#[derive(Debug, Default)]
pub struct AppearanceWatch {
    last: Option<bool>,
    failing: bool,
}

impl AppearanceWatch {
    /// The appearance to apply (true: dark) when `read` differs from the last one applied, or is
    /// the first; `None` when it is unchanged or the read failed.
    pub fn observe(&mut self, read: anyhow::Result<bool>) -> Option<bool> {
        match read {
            Ok(dark) => {
                if self.failing {
                    self.failing = false;
                    info!("the system appearance can be read again");
                }
                if self.last == Some(dark) {
                    return None;
                }
                self.last = Some(dark);
                Some(dark)
            }
            Err(error) => {
                let error = format!("{error:#}");
                if self.failing {
                    debug!(error = %error, "the system appearance still cannot be read");
                } else {
                    self.failing = true;
                    // Best-effort: the window keeps the appearance it has, and the next tick
                    // reads again.
                    warn!(error = %error, "could not read the system appearance");
                }
                None
            }
        }
    }
}

/// Stores `dark` (true: dark) in `current`, the appearance a window draws in once the system's
/// was read; true when that changed it.
pub fn set_appearance(current: &mut Option<bool>, dark: bool) -> bool {
    if *current == Some(dark) {
        return false;
    }
    *current = Some(dark);
    true
}

/// Reads the system appearance on each poll.
#[derive(Default)]
pub struct SystemAppearance {
    #[cfg(windows)]
    watch: AppearanceWatch,
    #[cfg(windows)]
    settings: Option<windows::UI::ViewManagement::UISettings>,
}

impl SystemAppearance {
    /// The appearance (true: dark) when it changed since the last poll. Always `None` off
    /// Windows, where Rune's own value stands.
    pub fn poll(&mut self) -> Option<bool> {
        #[cfg(windows)]
        {
            let read = self.read();
            self.watch.observe(read)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    #[cfg(windows)]
    fn read(&mut self) -> anyhow::Result<bool> {
        use anyhow::Context;
        use windows::UI::ViewManagement::{UIColorType, UISettings};

        let settings = match self.settings.take() {
            Some(settings) => settings,
            None => UISettings::new().context("Failed to create the UI settings")?,
        };
        let foreground = settings.GetColorValue(UIColorType::Foreground);
        self.settings = Some(settings);
        let foreground = foreground.context("Failed to read the system's foreground color")?;
        Ok(is_light(foreground.R, foreground.G, foreground.B))
    }
}

#[cfg(test)]
mod tests;
