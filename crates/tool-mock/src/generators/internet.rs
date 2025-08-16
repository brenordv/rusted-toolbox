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
            let idx = rand::thread_rng().gen_range(0..chars.len());
            chars.chars().nth(idx).unwrap()
        })
        .collect();
    
    Ok(password)
}

/// Generate a random URL
pub fn generate_url(_options: &MockOptions) -> Result<String> {
    let protocols = ["http", "https"];
    let protocol = protocols[rand::thread_rng().gen_range(0..protocols.len())];
    let domain = DomainSuffix().fake::<String>();
    let path_segments: Vec<String> = (0..rand::thread_rng().gen_range(1..4))
        .map(|_| FreeEmailProvider().fake::<String>().to_lowercase())
        .collect();
    
    Ok(format!("{}://www.{}.com/{}", protocol, domain.to_lowercase(), path_segments.join("/")))
}

/// Generate a random image URL
pub fn generate_image_url(_options: &MockOptions) -> Result<String> {
    let width = rand::thread_rng().gen_range(200..1200);
    let height = rand::thread_rng().gen_range(200..1200);
    
    Ok(format!("https://picsum.photos/{}/{}", width, height))
}

/// Generate a random file URL
pub fn generate_file_url(_options: &MockOptions) -> Result<String> {
    let protocols = ["http", "https", "ftp"];
    let protocol = protocols[rand::thread_rng().gen_range(0..protocols.len())];
    let domain = DomainSuffix().fake::<String>();
    let extensions = ["pdf", "doc", "docx", "txt", "jpg", "png", "zip", "tar.gz"];
    let extension = extensions[rand::thread_rng().gen_range(0..extensions.len())];
    let filename = FreeEmailProvider().fake::<String>().to_lowercase();
    
    Ok(format!("{}://files.{}.com/downloads/{}.{}", protocol, domain.to_lowercase(), filename, extension))
}
