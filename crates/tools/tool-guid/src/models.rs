/// Configuration for GUID generation operations.
///
/// Carries the clipboard flag, the empty-guid flag, and the optional
/// multiple-generation count.
pub struct GuidConfig {
    pub add_to_clipboard: bool,
    pub generate_empty_guid: bool,
    pub generate_multiple: Option<usize>,
}
