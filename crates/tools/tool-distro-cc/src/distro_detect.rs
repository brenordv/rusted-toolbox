use crate::models::DistroFamily;
use anyhow::{Context, Result};
use std::fs;

pub fn normalize_distro_name(input: &str) -> Option<DistroFamily> {
    let value = input.trim().to_lowercase();

    if value.is_empty() {
        return None;
    }

    if value.contains("debian")
        || value.contains("ubuntu")
        || value.contains("mint")
        || value.contains("pop")
        || value.contains("kali")
        || value.contains("raspbian")
        || value.contains("elementary")
        || value.contains("apt")
    {
        return Some(DistroFamily::Debian);
    }

    if value.contains("arch")
        || value.contains("manjaro")
        || value.contains("endeavour")
        || value.contains("artix")
        || value.contains("garuda")
        || value.contains("pacman")
    {
        return Some(DistroFamily::Arch);
    }

    None
}

pub fn detect_target_distro() -> Result<DistroFamily> {
    if std::env::consts::OS != "linux" {
        return Err(anyhow::anyhow!(
            "Unable to detect target distro on non-Linux systems. Please provide --to."
        ));
    }

    let content = read_os_release().context("Failed to read /etc/os-release")?;
    parse_os_release_content(&content).context("Unable to detect distro from /etc/os-release")
}

fn read_os_release() -> Result<String> {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        return Ok(content);
    }

    fs::read_to_string("/usr/lib/os-release").context("Failed to read /usr/lib/os-release")
}

fn parse_os_release_content(content: &str) -> Option<DistroFamily> {
    let mut id = None;
    let mut id_like = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }

        let mut parts = line.splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim().trim_matches('"').to_lowercase();

        match key {
            "ID" => id = Some(value),
            "ID_LIKE" => id_like = Some(value),
            _ => {}
        }
    }

    let combined = format!(
        "{} {}",
        id.clone().unwrap_or_default(),
        id_like.unwrap_or_default()
    );

    normalize_distro_name(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_os_release_debian() {
        let content = r#"
ID=ubuntu
ID_LIKE=debian
"#;
        let distro = parse_os_release_content(content);
        assert_eq!(distro, Some(DistroFamily::Debian));
    }

    #[test]
    fn parse_os_release_arch() {
        let content = r#"
ID=arch
ID_LIKE=archlinux
"#;
        let distro = parse_os_release_content(content);
        assert_eq!(distro, Some(DistroFamily::Arch));
    }
}
