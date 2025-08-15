/// Configuration for GUID generation operations.
///
/// Defines behavior for clipboard copying, empty GUID generation, output mode, and intervals.
pub struct GuidArgs {
    pub add_to_clipboard: bool,
    pub generate_empty_guid: bool,
    pub silent: bool,
    pub generate_on_interval: Option<f64>,
}
