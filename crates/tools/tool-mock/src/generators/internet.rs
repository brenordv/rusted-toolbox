use crate::models::MockOptions;
use anyhow::Result;
use fake::faker::internet::en::*;
use fake::Fake;
use rand::Rng;

/// Generate a random username
pub fn generate_username(_options: &MockOptions) -> Result<String> {
    Ok(Username().fake::<String>())
}

/// Generate a random password
pub fn generate_password(options: &MockOptions) -> Result<String> {
    let length = options.length.unwrap_or(12);

    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";
    let password: String = (0..length)
        .map(|_| {
            let idx = rand::rng().random_range(0..chars.len());
            chars.chars().nth(idx).unwrap()
        })
        .collect();

    Ok(password)
}

/// Generate a random URL
pub fn generate_url(_options: &MockOptions) -> Result<String> {
    let protocols = ["http", "https"];
    let protocol = protocols[rand::rng().random_range(0..protocols.len())];
    let domain = DomainSuffix().fake::<String>();
    let path_segments: Vec<String> = (0..rand::rng().random_range(1..4))
        .map(|_| FreeEmailProvider().fake::<String>().to_lowercase())
        .collect();

    Ok(format!(
        "{}://www.{}.com/{}",
        protocol,
        domain.to_lowercase(),
        path_segments.join("/")
    ))
}

/// Generate a random image URL
pub fn generate_image_url(_options: &MockOptions) -> Result<String> {
    let width = rand::rng().random_range(200..1200);
    let height = rand::rng().random_range(200..1200);

    Ok(format!("https://picsum.photos/{}/{}", width, height))
}

/// Generate a random file URL
pub fn generate_file_url(_options: &MockOptions) -> Result<String> {
    let protocols = ["http", "https", "ftp"];
    let protocol = protocols[rand::rng().random_range(0..protocols.len())];
    let domain = DomainSuffix().fake::<String>();
    let extensions = ["pdf", "doc", "docx", "txt", "jpg", "png", "zip", "tar.gz"];
    let extension = extensions[rand::rng().random_range(0..extensions.len())];
    let filename = FreeEmailProvider().fake::<String>().to_lowercase();

    Ok(format!(
        "{}://files.{}.com/downloads/{}.{}",
        protocol,
        domain.to_lowercase(),
        filename,
        extension
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DataType;

    const ALLOWED_PASSWORD_CHARS: &str =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";

    fn options() -> MockOptions {
        MockOptions {
            data_type: DataType::Username,
            min: None,
            max: None,
            length: None,
            precision: None,
            age: None,
            past: false,
            future: false,
            range: None,
        }
    }

    #[test]
    fn generate_password_uses_default_length() {
        let password = generate_password(&options()).unwrap();
        assert_eq!(password.chars().count(), 12);
    }

    #[test]
    fn generate_password_respects_custom_length_and_charset() {
        let mut opts = options();
        opts.length = Some(20);
        let password = generate_password(&opts).unwrap();
        assert_eq!(password.chars().count(), 20);
        assert!(password.chars().all(|c| ALLOWED_PASSWORD_CHARS.contains(c)));
    }

    #[test]
    fn generate_image_url_has_expected_shape() {
        let url = generate_image_url(&options()).unwrap();
        let dims: Vec<u32> = url
            .strip_prefix("https://picsum.photos/")
            .unwrap()
            .split('/')
            .map(|p| p.parse().unwrap())
            .collect();
        assert_eq!(dims.len(), 2);
        assert!(dims.iter().all(|d| (200..1200).contains(d)));
    }

    #[test]
    fn generate_url_is_http_and_dot_com() {
        let url = generate_url(&options()).unwrap();
        assert!(url.starts_with("http://www.") || url.starts_with("https://www."));
        assert!(url.contains(".com/"));
    }

    #[test]
    fn generate_file_url_contains_files_host_and_downloads() {
        let url = generate_file_url(&options()).unwrap();
        assert!(url.contains("://files."));
        assert!(url.contains("/downloads/"));
    }
}
