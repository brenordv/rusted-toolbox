use std::path::PathBuf;

/// Where an operation's output goes, resolved from `-o` at parse time.
/// `Derived` means no `-o` was given: a file input derives its sibling name
/// through `shared-crypto`'s naming rule, and a stdin input streams to stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputSpec {
    Derived,
    Stdout,
    Path(PathBuf),
}

/// The resolved job for one `seal` invocation.
#[derive(Debug, Clone)]
pub enum SealCommand {
    Keygen(KeygenJob),
    Encrypt(EncryptJob),
    Decrypt(DecryptJob),
    Watch,
}

/// `seal keygen`: write an identity file (printing the public key), or dump
/// the identity to stdout when no output file was given.
#[derive(Debug, Clone)]
pub struct KeygenJob {
    pub output: Option<PathBuf>,
    pub force: bool,
}

/// `seal encrypt`: seal a file or stdin to the merged recipient set. The
/// recipient literals and files stay unparsed here; the app parses them so
/// failures flow through the normal error path.
#[derive(Debug, Clone)]
pub struct EncryptJob {
    pub recipients: Vec<String>,
    pub recipient_files: Vec<PathBuf>,
    pub armor: bool,
    pub input: Option<PathBuf>,
    pub output: OutputSpec,
    pub force: bool,
}

/// `seal decrypt`: open an age file (or stdin) with the given identity files.
#[derive(Debug, Clone)]
pub struct DecryptJob {
    pub identity_files: Vec<PathBuf>,
    pub input: Option<PathBuf>,
    pub output: OutputSpec,
    pub force: bool,
}
