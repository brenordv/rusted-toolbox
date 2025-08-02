use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Precompiled regexes for detecting non-main or special multi-part patterns.
static RE_R_XX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\.r\d{2}$").expect("valid regex")); // .r00, .r01, etc. (legacy RAR continuation)
static RE_PART_N_RAR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\.part0*([1-9][0-9]*)\.rar$").expect("valid regex")); // .part01.rar etc.
static RE_Z_NN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\.z\d{2,}$").expect("valid regex")); // .z01, .z02 (ZIP splits)
static RE_7Z_VOL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\.7z\.(\d{3,})$").expect("valid regex")); // .7z.001, .7z.002

static COMPRESSED_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "zip", "gz", "bz2", "xz", "7z", "rar", "tar", "tgz", "tbz2", "lz4", "zst", "tar.gz",
        "tar.bz2", "tar.xz", "tar.zst", "tar.lz4", "cbr", "cbz", "ace", "arj", "lha", "jar",
    ]
    .iter()
    .cloned()
    .collect()
});

static IMAGE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "heif", "heic", "avif", "ico",
        "svg",
    ]
    .iter()
    .cloned()
    .collect()
});

static SUBTITLE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "srt", // SubRip
        "sub", // MicroDVD, SubViewer, or VobSub (paired with .idx)
        "ass", // Advanced SubStation Alpha
        "ssa", // SubStation Alpha
        "vtt", // WebVTT
        "sbv", // YouTube SBV
        "txt", // Plain text (sometimes used)
        "mpl", // MPL2
        "dks", // DKS
        "lrc", // Lyric subtitle (karaoke)
        "idx", // VobSub index file, paired with .sub
        "stl", // Spruce subtitle format (DVD authoring)
        "xml", // Sometimes used for subtitles, e.g., TTML/DFXP
    ]
    .iter()
    .cloned()
    .collect()
});

fn looks_like_archive_part(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    let multi_part_patterns = [
        // RAR volumes: .r00, .r01, etc.
        regex::Regex::new(r"\.r\d{2}$").unwrap(),
        // RAR style .partNN.rar
        regex::Regex::new(r"\.part\d+\.rar$").unwrap(),
        // ZIP split: .z01, .z02, etc.
        regex::Regex::new(r"\.z\d{2,}$").unwrap(),
        // 7z multi-volume: .7z.001 etc.
        regex::Regex::new(r"\.7z\.\d{3,}$").unwrap(),
        // Legacy splits like .aa, .ab (optional / conservative)
        regex::Regex::new(r"\.[a-z]{2}$").unwrap(),
    ];

    multi_part_patterns.iter().any(|re| re.is_match(&name))
}

pub trait PathBufExtensions {
    fn next_available_file(&self) -> Result<PathBuf>;
    fn is_compressed(&self) -> bool;
    fn is_main_file_multi_part_compression(&self) -> bool;
    fn absolute_to_string(&self) -> Result<String>;
    fn is_image(&self) -> bool;
    fn is_subtitle(&self) -> bool;
}

impl PathBufExtensions for PathBuf {
    fn next_available_file(&self) -> Result<PathBuf> {
        // 1. Check if the file exists.
        if !self.exists() {
            // 2. If it doesn't, return self and we're done.
            return Ok(self.clone());
        }

        // 3. If it exists, continue with the logic...

        let ext = self.extension().and_then(|e| e.to_str());

        let mut candidate = self.clone();

        loop {
            let stem = candidate
                .file_stem()
                .and_then(|s| s.to_str())
                .context("Failed to extract file stem")?;

            // Check if filename ends with a number
            if let Some((base, num)) = extract_number_suffix(stem) {
                // 3. If the filename ends with a number, increase this number.
                let new_num = num + 1;
                let new_filename = match ext {
                    Some(e) => format!("{}_{}.{}", base, new_num, e),
                    None => format!("{}_{}", base, new_num),
                };
                candidate = self.with_file_name(new_filename);
            } else {
                // 4. If the filename does not end with a number, add _1 as a suffix before the file extension.
                let new_filename = match ext {
                    Some(e) => format!("{}_1.{}", stem, e),
                    None => format!("{}_1", stem),
                };
                candidate = self.with_file_name(new_filename);
            }

            // 5. With the new filename, try again from 1.
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    fn is_compressed(&self) -> bool {
        self.as_path().is_compressed()
    }

    fn is_main_file_multi_part_compression(&self) -> bool {
        self.as_path().is_main_file_multi_part_compression()
    }

    fn absolute_to_string(&self) -> Result<String> {
        self.as_path().absolute_to_string()
    }

    fn is_image(&self) -> bool {
        self.as_path().is_image()
    }

    fn is_subtitle(&self) -> bool {
        self.as_path().is_subtitle()
    }
}

/// Extracts base and numeric suffix, if present.
/// Returns (base, num). E.g. "report_2" -> ("report", 2)
fn extract_number_suffix(stem: &str) -> Option<(&str, usize)> {
    let parts: Vec<&str> = stem.rsplitn(2, '_').collect();
    if parts.len() == 2 {
        if let Ok(num) = parts[0].parse() {
            return Some((parts[1], num));
        }
    }
    None
}

impl PathBufExtensions for Path {
    fn next_available_file(&self) -> Result<PathBuf> {
        self.to_path_buf().next_available_file()
    }

    fn is_compressed(&self) -> bool {
        // Quick filename-based heuristic (cheap). Lowercase for case-insensitive matching.
        let file_name = match self.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_ascii_lowercase(),
            None => return false,
        };

        // Check extension matches any known compressed extension (longest first).
        let mut extension_indicates = false;
        for &ext in COMPRESSED_EXTENSIONS.iter() {
            let pattern = format!(".{}", ext); // match e.g. ".tar.gz" or ".zip"
            if file_name.ends_with(&pattern) {
                extension_indicates = true;
                break;
            }
        }

        let looks_like_multi_part_archive = looks_like_archive_part(file_name.as_str());

        // Trying magic-byte detection. Doing this regardless, so we can catch archives
        // without or with weird extensions; but if the extension already says compressed,
        // we treat magic bytes as confirmation if they agree, else fall back to extension.
        match infer::get_from_path(self) {
            Ok(Some(kind)) => {
                let mime = kind.mime_type().to_ascii_lowercase();

                let magic_suggests = mime.starts_with("application/x-")
                    || mime.contains("zip")
                    || mime.contains("compressed")
                    || mime.contains("rar")
                    || mime.contains("tar");

                debug!("Mime Type for file: [{}][{}] ", mime, self.display());

                if magic_suggests || extension_indicates || looks_like_multi_part_archive {
                    return true;
                }

                false
            }
            // Any error reading the file / inferring: fall back to extension heuristic.
            Err(_) => {
                if extension_indicates || looks_like_multi_part_archive {
                    true
                } else {
                    // We couldn't inspect magic bytes and extension didn't trigger.
                    // Return false but wrap with context if caller wants to know about failures.
                    // We don't treat the error as fatal to keep function usable in best-effort mode.
                    false
                }
            }
            _ => false,
        }
    }

    /// Returns true if the path is the *main* file in a multi-part or standalone compressed archive.
    /// Continuation pieces like `.r01`, `.z02`, `.part2.rar`, `.7z.002`, or generic split chunks like `foo.zip.001`
    /// are rejected.
    fn is_main_file_multi_part_compression(&self) -> bool {
        let file_name = match self.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_ascii_lowercase(),
            None => return false,
        };

        // 1. Legacy RAR continuation volumes: .r00, .r01, etc. => not main.
        if RE_R_XX.is_match(&file_name) {
            return false;
        }

        // 2. RAR-style .partNN.rar:
        if let Some(caps) = RE_PART_N_RAR.captures(&file_name) {
            // Extract the numeric part; only part1 (or part01, etc) is considered main.
            if let Some(m) = caps.get(1) {
                if m.as_str() == "1" || m.as_str().trim_start_matches('0') == "1" {
                    return true; // .part1.rar or .part01.rar
                } else {
                    return false; // .part2.rar, .part03.rar, etc.
                }
            }
        }

        // 3. ZIP split volumes like .z01, .z02 => not main.
        if RE_Z_NN.is_match(&file_name) {
            return false;
        }

        // 4. 7z multi-volume: only .7z.001 is the “main” first volume.
        if let Some(caps) = RE_7Z_VOL.captures(&file_name) {
            if let Some(vol_num) = caps.get(1) {
                return vol_num.as_str() == "001";
            }
        }

        // 5. Generic split patterns like foo.zip.001 should not be treated as main.
        //    Reject if the name matches `*.zip.\d+` (common splitter output).
        if file_name
            .rsplitn(2, '.')
            .collect::<Vec<_>>()
            .get(1)
            .map_or(false, |base_ext| base_ext.ends_with("zip"))
            && file_name
                .split('.')
                .rev()
                .next()
                .map_or(false, |suffix| suffix.chars().all(|c| c.is_ascii_digit()))
        {
            return false;
        }

        // 6. Fallback to extension membership: if the filename ends with a known compressed extension,
        // and none of the above disqualifiers applied, consider it the main file.
        for &ext in COMPRESSED_EXTENSIONS.iter() {
            if file_name.ends_with(ext) {
                return true;
            }
        }

        false
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

    fn is_image(&self) -> bool {
        let is_image = if let Ok(Some(kind)) = infer::get_from_path(self) {
            kind.mime_type().starts_with("image/")
        } else {
            false
        };

        if is_image {
            return true;
        }

        // In case we cannot identify using the magic bytes, we fall back to checking the extension.
        self.extension()
            .and_then(|e| e.to_str())
            .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn is_subtitle(&self) -> bool {
        self.extension()
            .and_then(|e| e.to_str())
            .map(|e| SUBTITLE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[fixture]
    fn temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[fixture]
    fn sample_png_bytes() -> Vec<u8> {
        // Minimal valid PNG header
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
            0x49, 0x48, 0x44, 0x52, // IHDR chunk type
            0x00, 0x00, 0x00, 0x01, // Width: 1
            0x00, 0x00, 0x00, 0x01, // Height: 1
            0x08, 0x02, 0x00, 0x00,
            0x00, // Bit depth, color type, compression, filter, interlace
            0x90, 0x77, 0x53, 0xDE, // CRC
        ]
    }

    #[fixture]
    fn sample_jpeg_bytes() -> Vec<u8> {
        // Minimal valid JPEG header
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, // JPEG SOI + APP0
            0x00, 0x10, // APP0 length
            0x4A, 0x46, 0x49, 0x46, 0x00, // "JFIF\0"
            0x01, 0x01, // Version
            0x01, // Units
            0x00, 0x48, 0x00, 0x48, // X/Y density
            0x00, 0x00, // Thumbnail dimensions
        ]
    }

    #[rstest]
    #[case("test.jpg", true)]
    #[case("test.jpeg", true)]
    #[case("test.png", true)]
    #[case("test.gif", true)]
    #[case("test.bmp", true)]
    #[case("test.tiff", true)]
    #[case("test.tif", true)]
    #[case("test.webp", true)]
    #[case("test.heif", true)]
    #[case("test.heic", true)]
    #[case("test.avif", true)]
    #[case("test.ico", true)]
    #[case("test.svg", true)]
    #[case("TEST.JPG", true)]
    #[case("Test.Png", true)]
    #[case("image.JPEG", true)]
    #[case("file.GIF", true)]
    #[case("test.txt", false)]
    #[case("test.doc", false)]
    #[case("test.pdf", false)]
    #[case("test.mp4", false)]
    #[case("test.zip", false)]
    #[case("test.exe", false)]
    #[case("test", false)]
    #[case("image.jpgg", false)]
    #[case("test.pn", false)]
    #[case("file.backup.jpg", true)]
    fn should_return_true_for_mixed_case_image_extensions(
        temp_dir: TempDir,
        #[case] filename: &str,
        #[case] expected: bool,
    ) {
        // Arrange
        let file_path = temp_dir.path().join(filename);
        File::create(&file_path).unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert_eq!(
            result, expected,
            "Expected {} to be recognized as an image: {}",
            filename, expected
        );
    }

    #[rstest]
    fn should_return_true_for_valid_png_content_regardless_of_extension(
        temp_dir: TempDir,
        sample_png_bytes: Vec<u8>,
    ) {
        // Arrange
        let file_path = temp_dir.path().join("test.txt"); // Wrong extension
        let mut file = File::create(&file_path).unwrap();
        file.write_all(&sample_png_bytes).unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            result,
            "Expected file with PNG content to be recognized as image despite .txt extension"
        );
    }

    #[rstest]
    fn should_return_true_for_valid_jpeg_content_regardless_of_extension(
        temp_dir: TempDir,
        sample_jpeg_bytes: Vec<u8>,
    ) {
        // Arrange
        let file_path = temp_dir.path().join("document.doc"); // Wrong extension
        let mut file = File::create(&file_path).unwrap();
        file.write_all(&sample_jpeg_bytes).unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            result,
            "Expected file with JPEG content to be recognized as image despite .doc extension"
        );
    }

    #[rstest]
    fn should_fallback_to_extension_when_content_detection_fails(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("test.png");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"This is not image content").unwrap(); // Invalid content

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            result,
            "Expected fallback to extension-based detection when content detection fails"
        );
    }

    #[rstest]
    fn should_return_false_for_non_image_content_and_non_image_extension(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"This is plain text content").unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            !result,
            "Expected false for non-image content with non-image extension"
        );
    }

    #[rstest]
    fn should_return_false_for_empty_file_with_non_image_extension(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("empty.txt");
        File::create(&file_path).unwrap(); // Empty file

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            !result,
            "Expected false for empty file with non-image extension"
        );
    }

    #[rstest]
    fn should_return_true_for_empty_file_with_image_extension(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("empty.png");
        File::create(&file_path).unwrap(); // Empty file

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            result,
            "Expected true for empty file with image extension (fallback behavior)"
        );
    }

    #[rstest]
    fn should_return_false_for_nonexistent_file_with_non_image_extension() {
        // Arrange
        let file_path = PathBuf::from("nonexistent.txt");

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            !result,
            "Expected false for nonexistent file with non-image extension"
        );
    }

    #[rstest]
    fn should_return_true_for_nonexistent_file_with_image_extension() {
        // Arrange
        let file_path = PathBuf::from("nonexistent.jpg");

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            result,
            "Expected true for nonexistent file with image extension (fallback behavior)"
        );
    }

    #[rstest]
    fn should_return_false_for_path_without_extension(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("filename_without_extension");
        File::create(&file_path).unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(!result, "Expected false for file without extension");
    }

    #[rstest]
    fn should_return_false_for_path_with_only_dot(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("filename.");
        File::create(&file_path).unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(!result, "Expected false for file with empty extension");
    }

    #[rstest]
    fn should_handle_unicode_filenames_with_image_extensions(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("画像ファイル.jpg"); // Japanese characters
        File::create(&file_path).unwrap();

        // Act
        let result = file_path.is_image();

        // Assert
        assert!(
            result,
            "Expected true for Unicode filename with image extension"
        );
    }

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
    #[case::two_digit_extension("backup.zip.01", false)]
    #[case::three_digit_extension("backup.zip.001", false)]
    #[case::single_digit_extension("archive.part1", false)]
    #[case::single_digit_end("music.2", false)]
    #[case::main_file_zip("backup.zip", true)]
    #[case::main_file_rar("video.rar", true)]
    #[case::not_a_part_file("notes.txt", false)]
    #[case::not_digits_at_end("file.zip.foo", false)]
    #[case::multiple_digits_at_end("split.42", false)]
    #[case::zero_digit("zero.0", false)]
    #[case::hidden_file(".hidden", false)]
    #[case::numeric_filename("123456", false)]
    #[case::filename_01("movie.part1.rar", true)]
    #[case::filename_02("movie.part01.rar", true)]
    #[case::false_positive_for_non_standard_archive_name_01("archive.002.rar", true)]
    #[case::false_positive_for_non_standard_archive_name_02("file.part2.zip", true)]
    #[case::invalid_multipart_name_01("archive.part", false)]
    #[case::invalid_multipart_name_01("file.abc", false)]
    #[case::unconvenitional_filename("justdigits.99", false)]
    fn test_is_main_file_multi_part_compression(#[case] filename: &str, #[case] expected: bool) {
        let path = Path::new(filename);
        assert_eq!(path.is_main_file_multi_part_compression(), expected);
    }

    #[rstest]
    fn should_return_original_path_when_file_does_not_exist(temp_dir: TempDir) {
        // Arrange
        let file_path = temp_dir.path().join("nonexistent.txt");

        // Act
        let result = file_path.next_available_file().unwrap();

        // Assert
        assert_eq!(result, file_path);
    }

    #[rstest]
    #[case::a("document.txt", "document_1.txt")]
    #[case::b("document_5.txt", "document_6.txt")]
    #[case::c("README", "README_1")]
    #[case::c("README_3", "README_4")]
    #[case::d("file_0.txt", "file_1.txt")]
    #[case::e("file_999.txt", "file_1000.txt")]
    #[case::f("my_test_file_2.txt", "my_test_file_3.txt")]
    #[case::g("my_test_file_.txt", "my_test_file__1.txt")]
    #[case::h(".hidden", ".hidden_1")]
    #[case::i(".hidden.txt", ".hidden_1.txt")]
    #[case::j("archive.tar.gz", "archive.tar_1.gz")]
    #[case::k("文档.txt", "文档_1.txt")]
    #[case::k("123", "123_1")]
    #[case::k("456.txt", "456_1.txt")]
    #[case::k("a.txt", "a_1.txt")]
    #[case::k("report_v2.pdf", "report_v2_1.pdf")]
    fn should_add_suffix_1_when_file_exists_without_number_suffix(
        temp_dir: TempDir,
        #[case] filename: &str,
        #[case] expected_file: &str,
    ) {
        // Arrange
        let original_path = temp_dir.path().join(filename);
        File::create(&original_path).unwrap();

        // Act
        let result = original_path.next_available_file().unwrap();

        // Assert
        let expected = temp_dir.path().join(expected_file);
        assert_eq!(result, expected);
        assert!(!result.exists());
    }

    #[rstest]
    fn should_find_next_available_when_multiple_files_exist(temp_dir: TempDir) {
        // Arrange
        let base_path = temp_dir.path().join("report.pdf");
        let file1 = temp_dir.path().join("report_1.pdf");
        let file2 = temp_dir.path().join("report_2.pdf");
        let file3 = temp_dir.path().join("report_3.pdf");

        File::create(&base_path).unwrap();
        File::create(&file1).unwrap();
        File::create(&file2).unwrap();
        File::create(&file3).unwrap();

        // Act
        let result = base_path.next_available_file().unwrap();

        // Assert
        let expected = temp_dir.path().join("report_4.pdf");
        assert_eq!(result, expected);
    }

    #[rstest]
    fn should_handle_very_long_filename(temp_dir: TempDir) {
        // Arrange
        let long_name = "a".repeat(200);
        let filename = format!("{}.txt", long_name);
        let original_path = temp_dir.path().join(&filename);
        File::create(&original_path).unwrap();

        // Act
        let result = original_path.next_available_file().unwrap();

        // Assert
        let expected_filename = format!("{}_1.txt", long_name);
        let expected = temp_dir.path().join(expected_filename);
        assert_eq!(result, expected);
        assert!(!result.exists());
    }

    #[rstest]
    fn should_handle_gap_in_numbered_sequence_starting_from_original(temp_dir: TempDir) {
        // Arrange
        let original_path = temp_dir.path().join("file.txt");
        let file2 = temp_dir.path().join("file_2.txt");
        let file4 = temp_dir.path().join("file_4.txt");

        File::create(&original_path).unwrap();
        // Skip file_1.txt - it doesn't exist
        File::create(&file2).unwrap();
        // Skip file_3.txt - it doesn't exist
        File::create(&file4).unwrap();

        // Act
        let result = original_path.next_available_file().unwrap();

        // Assert
        let expected = temp_dir.path().join("file_1.txt");
        assert_eq!(result, expected);
        assert!(!result.exists());
    }
}
