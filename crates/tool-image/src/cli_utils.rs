use crate::models::EditArgs;
use crate::string_traits::StringExt;
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;

pub fn print_runtime_info(args: &EditArgs) {
    println!("🎨 Image v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    println!("Files");
    for file in args.input_files.iter() {
        println!("  {:?}", file);
    }

    if let Some(resize) = args.resize {
        println!("Resize: {}%", resize);
    }

    if args.grayscale {
        println!("Grayscale: true");
    }

    if let Some(convert) = &args.convert {
        println!("Convert: {:?}", convert);
    }

    println!();
}

pub fn validate_args(args: &EditArgs) -> Result<()> {
    if args.input_files.is_empty() {
        return Err(anyhow::anyhow!("No input files provided"));
    };

    if args.input_files.iter().any(|file| !file.exists()) {
        return Err(anyhow::anyhow!("Some of the input file does not exist"));
    }

    Ok(())
}

pub fn get_cli_arguments() -> EditArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Cli tool to perform some quick image edits."
        )
        .arg(Arg::new("input-files")
            .help("Input files to process")
            .num_args(0..)
            .required(false)
            .action(clap::ArgAction::Append))
        .arg(Arg::new("resize")
            .long("resize")
            .short('r')
            .value_name("resize")
            .value_parser(clap::value_parser!(u32))
            .help("Resize the image to a given percentage. (Example: 50=half size or 50%. 150=150% size)"))
        .arg(Arg::new("grayscale")
            .long("grayscale")
            .short('g')
            .action(clap::ArgAction::SetTrue)
            .help("Converts the image to grayscale. (Default: false)"))
        .arg(Arg::new("convert")
            .long("convert")
            .short('c')
            .value_name("FORMAT")
            .value_parser(clap::value_parser!(String))
            .help("Convert the image to the specified format"))
        .get_matches();

    let convert = if let Some(convert) = matches.get_one::<String>("convert") {
        Some(convert.to_image_format())
    } else {
        None
    };

    EditArgs {
        input_files: matches
            .get_many::<String>("input-files")
            .unwrap_or_default()
            .map(|s| s.into())
            .collect(),
        resize: matches.get_one::<u32>("resize").copied(),
        grayscale: matches.get_flag("grayscale"),
        convert,
    }
}
