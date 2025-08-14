use clap::ArgMatches;

/// Command-line arguments for cat utility.
///
/// Contains flags for cat options and list of files to process.
#[derive(Debug)]
pub struct CatArgs {
    show_all: bool,
    number_nonblank: bool,
    e_flag: bool,
    show_ends: bool,
    number: bool,
    squeeze_blank: bool,
    t_flag: bool,
    show_tabs: bool,
    pub u_flag: bool,
    show_nonprinting: bool,
    pub files: Vec<String>,
}

impl CatArgs {
    /// Parses command-line arguments from clap matches.
    ///
    /// Extracts flags and file list from parsed arguments.
    pub fn parse(args: &ArgMatches) -> Self {
        CatArgs {
            show_all: args.get_flag("show-all"),
            number_nonblank: args.get_flag("number-nonblank"),
            e_flag: args.get_flag("e"),
            show_ends: args.get_flag("show-ends"),
            number: args.get_flag("number"),
            squeeze_blank: args.get_flag("squeeze-blank"),
            t_flag: args.get_flag("t"),
            show_tabs: args.get_flag("show-tabs"),
            u_flag: args.get_flag("u"),
            show_nonprinting: args.get_flag("show-nonprinting"),
            files: args
                .get_many::<String>("files")
                .unwrap_or_default()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

/// Configuration options for cat text processing.
///
/// Controls output formatting including line numbering and character visualization.
#[derive(Debug)]
pub struct CatOptions {
    pub number_nonblank: bool,
    pub show_ends: bool,
    pub number: bool,
    pub squeeze_blank: bool,
    pub show_tabs: bool,
    pub show_nonprinting: bool,
}

impl CatOptions {
    /// Creates CatOptions from command-line arguments.
    ///
    /// Processes flag combinations where `-b` overrides `-n`, and composite flags like `-A`.
    pub fn from_args(args: &CatArgs) -> Self {
        let mut opts = CatOptions {
            number_nonblank: args.number_nonblank,
            show_ends: args.show_ends,
            number: args.number,
            squeeze_blank: args.squeeze_blank,
            show_tabs: args.show_tabs,
            show_nonprinting: args.show_nonprinting,
        };

        // Handle flag combinations
        if args.show_all {
            // -A is equivalent to -vET
            opts.show_nonprinting = true;
            opts.show_ends = true;
            opts.show_tabs = true;
        }

        if args.e_flag {
            // -e is equivalent to -vE
            opts.show_nonprinting = true;
            opts.show_ends = true;
        }

        if args.t_flag {
            // -t is equivalent to -vT
            opts.show_nonprinting = true;
            opts.show_tabs = true;
        }

        // -b implies -n but overrides it
        if opts.number_nonblank {
            opts.number = true;
        }

        opts
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