use anyhow::Context;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

static COMPRESSED_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "zip", "gz", "bz2", "xz", "7z", "rar", "tar", "tgz", "tbz2", "lz4", "zst", "tar.gz",
        "tar.bz2", "tar.xz", "tar.zst", "tar.lz4",
    ]
    .iter()
    .cloned()
    .collect()
});

pub trait PathBufExtensions {
    fn is_compressed(&self) -> bool;
    fn is_main_file_multi_part_compression(&self) -> bool;
    fn absolute_to_string(&self) -> anyhow::Result<String>;
}

impl PathBufExtensions for PathBuf {
    fn is_compressed(&self) -> bool {
        self.as_path().is_compressed()
    }

    fn is_main_file_multi_part_compression(&self) -> bool {
        self.as_path().is_main_file_multi_part_compression()
    }

    fn absolute_to_string(&self) -> anyhow::Result<String> {
        self.as_path().absolute_to_string()
    }
}

impl PathBufExtensions for Path {
    fn is_compressed(&self) -> bool {
        // Get filename as a string
        let file_name = match self.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_ascii_lowercase(),
            None => return false,
        };

        let parts: Vec<&str> = file_name.split('.').collect();
        for i in 1..parts.len() {
            let ext = parts[i..].join(".");
            if COMPRESSED_EXTENSIONS.contains(ext.as_str()) {
                return true;
            }
        }
        false
    }

    fn is_main_file_multi_part_compression(&self) -> bool {
        self.file_name()
            .and_then(|n| n.to_str())
            .map(|name| {
                name.chars().rev().take(2).all(|c| c.is_ascii_digit())
                    || name.chars().rev().take(1).all(|c| c.is_ascii_digit())
            })
            .unwrap_or(false)
    }

    fn absolute_to_string(&self) -> anyhow::Result<String> {
        match self
            .canonicalize()
            .context("Failed to canonicalize path")?
            .to_str()
        {
            Some(path) => Ok(path.to_string()),
            None => Err(anyhow::anyhow!("Path is not valid UTF-8")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("file.zip")]
    #[case("file.gz")]
    #[case("file.bz2")]
    #[case("file.xz")]
    #[case("file.7z")]
    #[case("file.rar")]
    #[case("file.tgz")]
    #[case("file.tbz2")]
    #[case("file.lz4")]
    #[case("file.zst")]
    #[case("file.tar")]
    #[case("archive.tar.gz")]
    #[case("archive.tar.bz2")]
    #[case("archive.tar.xz")]
    #[case("archive.tar.zst")]
    #[case("archive.tar.lz4")]
    fn test_is_compressed_compressed_extensions(#[case] filename: &str) {
        let path = Path::new(filename);
        assert!(
            path.is_compressed(),
            "Expected '{}' to be detected as compressed",
            filename
        );
        assert!(
            PathBuf::from(filename).is_compressed(),
            "Expected '{}' to be detected as compressed (PathBuf)",
            filename
        );
    }

    #[rstest]
    #[case("DATA.ZIP")]
    #[case("backup.Gz")]
    #[case("Archive.TAR.BZ2")]
    #[case("movie.RaR")]
    #[case("report.TGZ")]
    fn test_is_compressed_case_insensitivity(#[case] filename: &str) {
        let path = Path::new(filename);
        assert!(
            path.is_compressed(),
            "Expected '{}' to be detected as compressed (case insensitivity)",
            filename
        );
    }

    #[rstest]
    #[case("document.txt")]
    #[case("photo.jpg")]
    #[case("music.mp3")]
    #[case("presentation.pptx")]
    #[case("video.mkv")]
    #[case("README")]
    #[case(".hiddenfile")]
    fn test_is_compressed_non_compressed_files(#[case] filename: &str) {
        let path = Path::new(filename);
        assert_eq!(
            path.is_compressed(),
            false,
            "Expected '{}' to not be detected as compressed",
            filename
        );
    }

    #[rstest]
    #[case("")]
    #[case(".")]
    #[case("..")]
    #[case("/some/random/path/")]
    #[case("folder/")]
    #[case("folder/file.")]
    fn test_is_compressed_directory_names_and_paths(#[case] dirname: &str) {
        // Directories or paths that are not files should return false
        let path = Path::new(dirname);
        assert!(
            !path.is_compressed(),
            "Expected '{}' to not be detected as compressed",
            dirname
        );
    }

    #[rstest]
    #[case(".tar.gz", true)] // hidden file but compressed extension
    #[case(".archive.tar.gz", true)] // hidden file but compressed extension
    #[case(".env", false)] // not compressed
    #[case(".normal.7z", true)] // compressed
    #[case("....tar.gz", true)] // not even sure if this is a file, but it is still compressed
    fn test_is_compressed_hidden_and_weird_filenames(
        #[case] filename: &str,
        #[case] expected: bool,
    ) {
        let path = Path::new(filename);
        assert_eq!(
            path.is_compressed(),
            expected,
            "Expected '{}' to {}be detected as compressed",
            filename,
            if expected { "" } else { "not " }
        );
    }

    #[rstest]
    #[case("my.backup.2023-07-01.tar.gz", true)]
    #[case("double.dot.tar.bz2", true)]
    #[case("multiple.dots.notcompressed.doc", false)]
    fn test_is_compressed_paths_with_multiple_dots(#[case] filename: &str, #[case] expected: bool) {
        let path = Path::new(filename);
        assert_eq!(
            path.is_compressed(),
            expected,
            "Expected '{}' to {}be detected as compressed",
            filename,
            if expected { "" } else { "not " }
        );
    }

    #[rstest]
    #[case("notcompressed.tarx")]
    #[case("almosttar.gzipped")]
    #[case("compressed.gzip")]
    #[case("compressed.tar.x")]
    fn test_is_compressed_files_with_similar_extensions(#[case] filename: &str) {
        let path = Path::new(filename);
        assert_eq!(
            path.is_compressed(),
            false,
            "Expected '{}' to not be detected as compressed",
            filename
        );
    }

    #[rstest]
    #[case::two_digit_extension("backup.zip.01", true)]
    #[case::three_digit_extension("backup.zip.001", true)]
    #[case::single_digit_extension("archive.part1", true)]
    #[case::single_digit_end("music.2", true)]
    #[case::main_file_zip("backup.zip", false)]
    #[case::main_file_rar("video.rar", false)]
    #[case::not_a_part_file("notes.txt", false)]
    #[case::not_digits_at_end("file.zip.foo", false)]
    #[case::multiple_digits_at_end("split.42", true)]
    #[case::zero_digit("zero.0", true)]
    #[case::hidden_file(".hidden", false)]
    #[case::numeric_filename("123456", true)]
    #[case::unsupported_filename_01("movie.part1.rar", false)]
    #[case::unsupported_filename_02("movie.part01.rar", false)]
    #[case::unsupported_filename_03("archive.001.rar", false)]
    #[case::unsupported_filename_04("file.part2.zip", false)]
    #[case::invalid_multipart_name_01("archive.part", false)]
    #[case::invalid_multipart_name_01("file.abc", false)]
    #[case::unconvenitional_filename("justdigits.99", true)]
    fn test_is_main_file_multi_part_compression(#[case] filename: &str, #[case] expected: bool) {
        let path = Path::new(filename);
        assert_eq!(path.is_main_file_multi_part_compression(), expected);
    }
}
