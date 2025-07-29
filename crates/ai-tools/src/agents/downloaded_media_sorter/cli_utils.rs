use shared::constants::general::DASH_LINE;
use crate::constants::{AI_DOWNLOADED_MEDIA_SORTER_VERSION};

pub fn print_runtime_info(watch_folder: &String) {
    println!("🧠 Downloaded Media Organizer v{}", AI_DOWNLOADED_MEDIA_SORTER_VERSION);
    println!("{}", DASH_LINE);

    println!("📁 Wacthing folder: {}", watch_folder);

    println!();
}