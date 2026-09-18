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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use rstest::rstest;

    fn parse(argv: &[&str]) -> CatConfig {
        CatConfig::from_args(&CliArgs::try_parse_from(argv).unwrap())
    }

    fn flagless() -> CatConfig {
        CatConfig {
            number_nonblank: false,
            show_ends: false,
            number: false,
            squeeze_blank: false,
            show_tabs: false,
            show_nonprinting: false,
            files: vec![],
        }
    }

    // Expected flags are [show_nonprinting, show_ends, show_tabs, number, number_nonblank].
    #[rstest]
    #[case::no_flags(&["cat"], [false, false, false, false, false])]
    #[case::show_all_implies_v_e_t(&["cat", "-A"], [true, true, true, false, false])]
    #[case::lowercase_e_implies_v_e(&["cat", "-e"], [true, true, false, false, false])]
    #[case::lowercase_t_implies_v_t(&["cat", "-t"], [true, false, true, false, false])]
    #[case::b_implies_numbering(&["cat", "-b"], [false, false, false, true, true])]
    #[case::b_overrides_n(&["cat", "-b", "-n"], [false, false, false, true, true])]
    fn from_args_expands_flag_combinations(#[case] argv: &[&str], #[case] expected: [bool; 5]) {
        let config = parse(argv);
        let [
            show_nonprinting,
            show_ends,
            show_tabs,
            number,
            number_nonblank,
        ] = expected;

        assert_eq!(config.show_nonprinting, show_nonprinting);
        assert_eq!(config.show_ends, show_ends);
        assert_eq!(config.show_tabs, show_tabs);
        assert_eq!(config.number, number);
        assert_eq!(config.number_nonblank, number_nonblank);
    }

    #[test]
    fn from_args_copies_files_and_squeeze_flag_through() {
        let config = parse(&["cat", "-s", "a.txt", "b.txt"]);

        assert!(config.squeeze_blank);
        assert_eq!(config.files, vec!["a.txt".to_string(), "b.txt".to_string()]);
    }

    #[rstest]
    #[case::number_nonblank(CatConfig { number_nonblank: true, ..flagless() })]
    #[case::show_ends(CatConfig { show_ends: true, ..flagless() })]
    #[case::number(CatConfig { number: true, ..flagless() })]
    #[case::squeeze_blank(CatConfig { squeeze_blank: true, ..flagless() })]
    #[case::show_tabs(CatConfig { show_tabs: true, ..flagless() })]
    #[case::show_nonprinting(CatConfig { show_nonprinting: true, ..flagless() })]
    fn needs_line_processing_is_true_when_any_formatting_flag_is_set(#[case] config: CatConfig) {
        assert!(config.needs_line_processing());
    }

    #[test]
    fn needs_line_processing_is_false_with_all_flags_off() {
        assert!(!flagless().needs_line_processing());
    }
}
