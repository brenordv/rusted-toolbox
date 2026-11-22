/// What one run of speak was asked to do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeakArgs {
    /// The text to speak; `None` reads it from a piped stdin.
    pub text: Option<String>,
    /// The model for this run, in place of the configured one.
    pub model: Option<String>,
    /// The voice for this run, in place of the configured one.
    pub voice: Option<String>,
}
