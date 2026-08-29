/// Configuration for GUID generation operations.
///
/// Defines behavior for clipboard copying, empty GUID generation, output mode, and intervals.
pub struct GuidConfig {
    pub add_to_clipboard: bool,
    pub generate_empty_guid: bool,
    pub generate_multiple: Option<usize>,
}
