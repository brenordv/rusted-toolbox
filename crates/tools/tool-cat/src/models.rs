use crate::cli_utils::CliArgs;

/// Command-line arguments for cat utility.
///
/// Contains flags for cat options and list of files to process.
#[derive(Debug)]
pub struct CatConfig {
    pub(crate) number_nonblank: bool,
    pub(crate) show_ends: bool,
    pub(crate) number: bool,
    pub(crate) squeeze_blank: bool,
    pub(crate) show_tabs: bool,
    pub(crate) show_nonprinting: bool,
    pub(crate) files: Vec<String>,
}

impl CatConfig {
    pub fn from_args(args: &CliArgs) -> Self {
        let mut config = CatConfig {
            number_nonblank: args.number_nonblank,
            show_ends: args.show_ends,
            number: args.number,
            squeeze_blank: args.squeeze_blank,
            show_tabs: args.show_tabs,
            show_nonprinting: args.show_nonprinting,
            files: args.files.clone(),
        };

        // Handle flag combinations
        if args.show_all {
            // -A is equivalent to -vET
            config.show_nonprinting = true;
            config.show_ends = true;
            config.show_tabs = true;
        }

        if args.e {
            // -e is equivalent to -vE
            config.show_nonprinting = true;
            config.show_ends = true;
        }

        if args.t {
            // -t is equivalent to -vT
            config.show_nonprinting = true;
            config.show_tabs = true;
        }

        // -b implies -n but overrides it
        if config.number_nonblank {
            config.number = true;
        }

        config
    }

    /// Determines if line processing is required.
    ///
    /// Returns true if any formatting options are enabled.
    pub fn needs_line_processing(&self) -> bool {
        self.number_nonblank
            || self.show_ends
            || self.number
            || self.squeeze_blank
            || self.show_tabs
            || self.show_nonprinting
    }
}
