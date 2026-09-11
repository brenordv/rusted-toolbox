/// Horizontal rule printed under a tool's name/version line in the
/// `--app-header` output.
pub const DASH_LINE: &str = "---------------------------------------------------";

/// Top-level bullet prefix of the runtime-config block in the `--app-header`
/// output.
pub const CONFIG_UL_ITEM_LEVEL_1: &str = "-";

/// Second-level (indented) bullet prefix of the runtime-config block.
pub const CONFIG_UL_ITEM_LEVEL_2: &str = "  -";

/// Third-level (indented) bullet prefix of the runtime-config block.
pub const CONFIG_UL_ITEM_LEVEL_3: &str = "    -";

/// 8 KiB in bytes, for sizing I/O buffers.
pub const SIZE_8KB: usize = 8 * 1024;

/// 16 KiB in bytes, for sizing I/O buffers.
pub const SIZE_16KB: usize = 16 * 1024;

/// 32 KiB in bytes, for sizing I/O buffers.
pub const SIZE_32KB: usize = 32 * 1024;

/// 64 KiB in bytes, for sizing I/O buffers.
pub const SIZE_64KB: usize = 64 * 1024;

/// 128 KiB in bytes, for sizing I/O buffers.
pub const SIZE_128KB: usize = 128 * 1024;

/// Exit code returned when the user interrupts a tool (e.g., Ctrl-C).
pub const EXIT_CODE_INTERRUPTED_BY_USER: i32 = 130;
