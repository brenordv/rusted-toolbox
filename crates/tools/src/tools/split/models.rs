/// Configuration for file splitting operations.
///
/// Contains input file, output directory, line count limits, and CSV mode settings.
pub struct SplitArgs {
    pub input_file: String,
    pub output_dir: String,
    pub input_filename_without_extension: String,
    pub lines_per_file: usize,
    pub prefix: String,
    pub csv_mode: bool,
    pub feedback_interval: usize,
}
