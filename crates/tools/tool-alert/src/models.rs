use std::path::PathBuf;

/// The command line as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertArgs {
    pub title: Option<String>,
    pub body: Option<String>,
    pub sound: Option<String>,
    pub gui: bool,
}

/// What one run of alert does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Shows the alert and waits for the user to close it.
    Show(AlertSpec),
    /// Opens the tester, its fields filled with these values.
    Tester(TesterSeed),
}

/// One alert: what it says and the sound it plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertSpec {
    /// The title, when one was given and it is not blank.
    pub title: Option<String>,
    /// The message. Never blank.
    pub body: String,
    /// The sound file to play when the alert shows.
    pub sound: Option<PathBuf>,
}

/// The text the tester's three fields start with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TesterSeed {
    pub title: String,
    pub body: String,
    pub sound: String,
}

/// How the alert window was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedBy {
    /// A pointer press, or an assistive client's click, on Close.
    Button,
    /// Return, Space on Close, or Escape.
    Key,
    /// The window's own close control.
    Window,
}

impl ClosedBy {
    /// The word logs use for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Key => "key",
            Self::Window => "window",
        }
    }
}
