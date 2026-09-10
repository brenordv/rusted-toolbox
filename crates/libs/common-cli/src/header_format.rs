//! Pure renderers for the `--app-header` block. They return strings instead of
//! printing, so the exact bytes every tool shows are pinned by the tests below
//! and `app_boot_up` only has to print the rendered result. The line renderers
//! are public so tool header printers build their own sections from the same
//! formatting instead of hand-assembling lines from the `CONFIG_UL_*` constants.

use common_utils::constants::{
    CONFIG_UL_ITEM_LEVEL_1, CONFIG_UL_ITEM_LEVEL_2, CONFIG_UL_ITEM_LEVEL_3, DASH_LINE,
};
use std::fmt::Display;

/// Renders one level-1 section line of the runtime-config block, e.g.
/// `- Tool Runtime Config`.
pub fn format_config_section(title: &str) -> String {
    format!("{CONFIG_UL_ITEM_LEVEL_1} {title}")
}

/// Renders one level-2 item line of the runtime-config block, e.g.
/// `  - Log level: Debug`.
pub fn format_config_item(label: &str, value: impl Display) -> String {
    format!("{CONFIG_UL_ITEM_LEVEL_2} {label}: {value}")
}

/// Renders one level-3 item line of the runtime-config block, e.g.
/// `    - Payload size: 56 bytes`. Used for details nested under a level-2 item.
pub fn format_config_item_level3(label: &str, value: impl Display) -> String {
    format!("{CONFIG_UL_ITEM_LEVEL_3} {label}: {value}")
}

/// Renders the standard header block: the `name (version)` title line, the
/// dashed rule, and the Basic Runtime Config section with its five items. No
/// trailing newline; the caller's `println!` supplies it.
pub(crate) fn render_standard_header(
    app_name: &str,
    app_version: &str,
    verbose: impl Display,
    log_level: impl Display,
    log_to_stdout: bool,
    log_to_file: bool,
    rotate_log_file_by_day: bool,
) -> String {
    [
        format!("{app_name} ({app_version})"),
        DASH_LINE.to_string(),
        format_config_section("Basic Runtime Config"),
        format_config_item("Verbose mode", verbose),
        format_config_item("Log level", log_level),
        format_config_item("Log to stdout", log_to_stdout),
        format_config_item("Log to file", log_to_file),
        format_config_item("Rotate log file by day", rotate_log_file_by_day),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_config_section_uses_level_1_prefix() {
        assert_eq!(
            format_config_section("Tool Runtime Config"),
            "- Tool Runtime Config"
        );
    }

    #[test]
    fn format_config_item_uses_level_2_prefix() {
        assert_eq!(
            format_config_item("Log level", "Debug"),
            "  - Log level: Debug"
        );
    }

    #[test]
    fn format_config_item_level3_uses_level_3_prefix() {
        assert_eq!(
            format_config_item_level3("Payload size", "56 bytes"),
            "    - Payload size: 56 bytes"
        );
    }

    #[test]
    fn render_standard_header_pins_exact_bytes() {
        let header = render_standard_header("b64", "2.0.0", true, "Warning", false, true, false);

        let expected = concat!(
            "b64 (2.0.0)\n",
            "---------------------------------------------------\n",
            "- Basic Runtime Config\n",
            "  - Verbose mode: true\n",
            "  - Log level: Warning\n",
            "  - Log to stdout: false\n",
            "  - Log to file: true\n",
            "  - Rotate log file by day: false",
        );
        assert_eq!(header, expected);
    }

    #[test]
    fn render_standard_header_renders_unused_verbose_placeholder() {
        let header = render_standard_header("cat", "1.0.0", "<unused>", "Info", true, false, true);

        assert!(header.contains("\n  - Verbose mode: <unused>\n"));
    }
}
