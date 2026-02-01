#![allow(dead_code, unused_doc_comments)]
use ai_macros::ai_function;

const OUTPUT: &str = "";

#[ai_function]
pub fn convert_distro_command(_command: &str, _from_distro: &str, _to_distro: &str) -> &'static str {
    /// ROLE
    /// - Translate one distro package manager command from `from_distro` to `to_distro`.
    /// - Output EXACTLY ONE converted command string with no commentary.
    ///
    /// INPUTS
    /// - `command`: a single command line (may include stray words or code fences; extract the command).
    /// - `from_distro`: source distro family name, e.g., "debian" or "arch".
    /// - `to_distro`: target distro family name, e.g., "debian" or "arch".
    ///
    /// CONVERSION STRATEGY (FOLLOW IN ORDER)
    /// 1) Normalize: strip code fences/backticks and extra whitespace.
    /// 2) Identify the base package manager and subcommand (install/remove/search/update/upgrade).
    /// 3) Map to the target distro command, preserving argument order.
    /// 4) If the source has no safe equivalent, return the original command unchanged.
    ///
    /// SAFETY
    /// - Do NOT add sudo or destructive flags unless present in the input.
    /// - Prefer minimal change: preserve argument order and spacing where possible.
    ///
    /// OUTPUT CONTRACT (STRICT)
    /// - Output MUST be exactly one command line.
    /// - No code fences, no explanations, no extra whitespace.
    /// - If unsure, return the original command unchanged.
    OUTPUT
}
