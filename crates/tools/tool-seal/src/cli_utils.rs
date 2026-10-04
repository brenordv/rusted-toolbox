use crate::models::{DecryptJob, EncryptJob, KeygenJob, OutputSpec, SealCommand};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use std::path::PathBuf;

/// Encrypts and decrypts files with key-based age encryption.
///
/// `keygen` creates an identity (key pair), `encrypt` seals a file or stdin to
/// one or more recipients, and `decrypt` opens sealed files with identity
/// files. Only key-based files are supported; passphrase-encrypted age files
/// are refused.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about, arg_required_else_help = true)]
struct CliArgs {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate a new identity (key pair)
    #[command(after_help = "Examples:\n  \
        seal keygen -o key.txt\n  \
        seal keygen -o key.txt --force\n  \
        seal keygen > key.txt")]
    Keygen(KeygenArgs),

    /// Encrypt a file or stdin to one or more recipients
    #[command(after_help = "Examples:\n  \
        seal encrypt -R team.txt report.pdf\n  \
        seal encrypt -r age1example... -o notes.age notes.txt\n  \
        tar cz docs | seal encrypt -R team.txt -o docs.tar.gz.age")]
    Encrypt(EncryptArgs),

    /// Decrypt an age file with one or more identity files
    #[command(after_help = "Examples:\n  \
        seal decrypt -i key.txt report.pdf.age\n  \
        seal decrypt -i key.txt -o - report.pdf.age | less")]
    Decrypt(DecryptArgs),

    /// Watch a folder and seal files as they appear (planned for a later release)
    Watch,
}

#[derive(Args, Debug)]
struct KeygenArgs {
    /// Write the identity file here and print just the public key;
    /// without it, the full identity file goes to stdout
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Overwrite an existing identity file
    #[arg(long = "force")]
    pub force: bool,
}

#[derive(Args, Debug)]
struct EncryptArgs {
    /// Recipient public key (age1...); may be given multiple times
    #[arg(short = 'r', long = "recipient", value_name = "AGE1...")]
    pub recipients: Vec<String>,

    /// File with one recipient per line (# comments and blank lines ignored); may be given multiple times
    #[arg(short = 'R', long = "recipients-file", value_name = "FILE")]
    pub recipient_files: Vec<PathBuf>,

    /// Write ASCII-armored output instead of the binary age format
    #[arg(long = "armor")]
    pub armor: bool,

    /// Output file; `-` forces stdout. Default: `<input>.age` next to the input, or stdout for stdin input
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Overwrite an existing output file
    #[arg(long = "force")]
    pub force: bool,

    /// File to encrypt; omit to read from stdin
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct DecryptArgs {
    /// Identity file to decrypt with; may be given multiple times
    #[arg(short = 'i', long = "identity", value_name = "FILE", required = true)]
    pub identity_files: Vec<PathBuf>,

    /// Output file; `-` forces stdout. Default: the input with one `.age` stripped, or stdout for stdin input
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Overwrite an existing output file
    #[arg(long = "force")]
    pub force: bool,

    /// File to decrypt; omit to read from stdin
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,
}

/// Parses command-line arguments, boots the tool, and returns the resolved
/// job. Recipient and identity parsing stay with the app so their failures
/// flow through the normal error path after logging is up.
pub fn initialize() -> Result<SealCommand> {
    let args = CliArgs::parse();

    let command = build_command(args.command);

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| print_header(&command)),
    );

    Ok(command)
}

/// Maps the parsed subcommand onto its resolved job struct.
fn build_command(command: Commands) -> SealCommand {
    match command {
        Commands::Keygen(args) => SealCommand::Keygen(KeygenJob {
            output: args.output,
            force: args.force,
        }),
        Commands::Encrypt(args) => SealCommand::Encrypt(EncryptJob {
            recipients: args.recipients,
            recipient_files: args.recipient_files,
            armor: args.armor,
            output: output_spec(args.output),
            force: args.force,
            input: args.input,
        }),
        Commands::Decrypt(args) => SealCommand::Decrypt(DecryptJob {
            identity_files: args.identity_files,
            output: output_spec(args.output),
            force: args.force,
            input: args.input,
        }),
        Commands::Watch => SealCommand::Watch,
    }
}

/// Maps `-o` onto the output spec: absent derives the default, `-` forces
/// stdout, anything else is a literal path.
fn output_spec(output: Option<PathBuf>) -> OutputSpec {
    match output {
        None => OutputSpec::Derived,
        Some(path) if path.as_os_str() == "-" => OutputSpec::Stdout,
        Some(path) => OutputSpec::Path(path),
    }
}

/// Prints the tool's runtime configuration, shown under `--app-header`. Key
/// material never appears here; paths and counts only.
fn print_header(command: &SealCommand) {
    match command {
        SealCommand::Keygen(job) => {
            println!("{}", format_config_item("Mode", "keygen"));
            let output = match &job.output {
                Some(path) => path.display().to_string(),
                None => "stdout".to_string(),
            };
            println!("{}", format_config_item("Output", output));
            println!("{}", format_config_item("Overwrite", job.force));
        }
        SealCommand::Encrypt(job) => {
            println!("{}", format_config_item("Mode", "encrypt"));
            println!("{}", format_config_item("Input", display_input(&job.input)));
            println!(
                "{}",
                format_config_item("Output", display_output(&job.output))
            );
            println!(
                "{}",
                format_config_item(
                    "Recipients",
                    format!(
                        "{} literal(s), {} file(s)",
                        job.recipients.len(),
                        job.recipient_files.len()
                    )
                )
            );
            println!("{}", format_config_item("Armor", job.armor));
            println!("{}", format_config_item("Overwrite", job.force));
        }
        SealCommand::Decrypt(job) => {
            println!("{}", format_config_item("Mode", "decrypt"));
            println!("{}", format_config_item("Input", display_input(&job.input)));
            println!(
                "{}",
                format_config_item("Output", display_output(&job.output))
            );
            println!(
                "{}",
                format_config_item("Identity files", job.identity_files.len())
            );
            println!("{}", format_config_item("Overwrite", job.force));
        }
        SealCommand::Watch => {
            println!("{}", format_config_item("Mode", "watch (reserved)"));
        }
    }
}

fn display_input(input: &Option<PathBuf>) -> String {
    match input {
        Some(path) => path.display().to_string(),
        None => "stdin".to_string(),
    }
}

fn display_output(output: &OutputSpec) -> String {
    match output {
        OutputSpec::Derived => "(derived from the input name)".to_string(),
        OutputSpec::Stdout => "stdout".to_string(),
        OutputSpec::Path(path) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(argv: &[&str]) -> CliArgs {
        CliArgs::try_parse_from(argv).unwrap()
    }

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn bare_invocation_is_rejected_with_help() {
        let error = CliArgs::try_parse_from(["seal"]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn keygen_maps_output_and_force() {
        let args = parse(&["seal", "keygen", "-o", "key.txt", "--force"]);

        let SealCommand::Keygen(job) = build_command(args.command) else {
            panic!("expected the keygen job");
        };
        assert_eq!(job.output, Some(PathBuf::from("key.txt")));
        assert!(job.force);
    }

    #[test]
    fn keygen_output_dash_stays_a_literal_path() {
        let args = parse(&["seal", "keygen", "-o", "-"]);

        let SealCommand::Keygen(job) = build_command(args.command) else {
            panic!("expected the keygen job");
        };
        assert_eq!(job.output, Some(PathBuf::from("-")));
    }

    #[test]
    fn encrypt_merges_repeatable_recipient_flags() {
        let args = parse(&[
            "seal", "encrypt", "-r", "age1aaa", "-r", "age1bbb", "-R", "team.txt", "--armor",
            "in.txt",
        ]);

        let SealCommand::Encrypt(job) = build_command(args.command) else {
            panic!("expected the encrypt job");
        };
        assert_eq!(job.recipients, ["age1aaa", "age1bbb"]);
        assert_eq!(job.recipient_files, [PathBuf::from("team.txt")]);
        assert!(job.armor);
        assert_eq!(job.input, Some(PathBuf::from("in.txt")));
        assert_eq!(job.output, OutputSpec::Derived);
    }

    #[test]
    fn output_dash_forces_stdout_and_a_path_stays_a_path() {
        assert_eq!(output_spec(Some(PathBuf::from("-"))), OutputSpec::Stdout);
        assert_eq!(
            output_spec(Some(PathBuf::from("out.age"))),
            OutputSpec::Path(PathBuf::from("out.age"))
        );
        assert_eq!(output_spec(None), OutputSpec::Derived);
    }

    #[test]
    fn decrypt_requires_at_least_one_identity_file() {
        let error = CliArgs::try_parse_from(["seal", "decrypt", "in.age"]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn decrypt_collects_identity_files_in_order() {
        let args = parse(&["seal", "decrypt", "-i", "a.txt", "-i", "b.txt", "in.age"]);

        let SealCommand::Decrypt(job) = build_command(args.command) else {
            panic!("expected the decrypt job");
        };
        assert_eq!(
            job.identity_files,
            [PathBuf::from("a.txt"), PathBuf::from("b.txt")]
        );
        assert_eq!(job.input, Some(PathBuf::from("in.age")));
    }

    #[test]
    fn watch_parses_bare() {
        let args = parse(&["seal", "watch"]);
        assert!(matches!(build_command(args.command), SealCommand::Watch));
    }

    #[test]
    fn display_helpers_name_stdin_and_derived_defaults() {
        assert_eq!(display_input(&None), "stdin");
        assert_eq!(display_input(&Some(PathBuf::from("a.txt"))), "a.txt");
        assert_eq!(display_output(&OutputSpec::Stdout), "stdout");
        assert_eq!(
            display_output(&OutputSpec::Derived),
            "(derived from the input name)"
        );
        assert_eq!(
            display_output(&OutputSpec::Path(PathBuf::from("x.age"))),
            "x.age"
        );
    }
}
