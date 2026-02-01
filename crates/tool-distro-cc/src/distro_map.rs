use crate::models::DistroFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalAction {
    Install,
    Remove,
    Search,
    Update,
    Upgrade,
    UpdateUpgrade,
}

#[derive(Debug)]
struct ParsedCommand {
    action: CanonicalAction,
    args: Vec<String>,
}

pub fn convert_parts_with_map(
    from: DistroFamily,
    to: DistroFamily,
    parts: &[String],
) -> Option<String> {
    let parsed = parse_command(from, parts)?;
    build_command(to, parsed)
}

fn parse_command(from: DistroFamily, parts: &[String]) -> Option<ParsedCommand> {
    if parts.is_empty() {
        return None;
    }

    match from {
        DistroFamily::Debian => parse_debian_command(parts),
        DistroFamily::Arch => parse_arch_command(parts),
    }
}

fn parse_debian_command(parts: &[String]) -> Option<ParsedCommand> {
    let base = parts.get(0)?.to_lowercase();
    if base != "apt" && base != "apt-get" {
        return None;
    }

    let action_token = parts.get(1)?.to_lowercase();
    if action_token.starts_with('-') {
        return None;
    }

    let args = parts.get(2..).unwrap_or(&[]).to_vec();
    if has_option_tokens(&args) {
        return None;
    }

    let action = match action_token.as_str() {
        "install" => CanonicalAction::Install,
        "remove" | "purge" => CanonicalAction::Remove,
        "search" => CanonicalAction::Search,
        "update" => CanonicalAction::Update,
        "upgrade" | "dist-upgrade" => CanonicalAction::Upgrade,
        _ => return None,
    };

    if matches!(action, CanonicalAction::Update | CanonicalAction::Upgrade) && !args.is_empty() {
        return None;
    }

    if matches!(
        action,
        CanonicalAction::Install | CanonicalAction::Remove | CanonicalAction::Search
    ) && args.is_empty()
    {
        return None;
    }

    Some(ParsedCommand { action, args })
}

fn parse_arch_command(parts: &[String]) -> Option<ParsedCommand> {
    let base = parts.get(0)?.to_lowercase();
    if base != "pacman" {
        return None;
    }

    let flag = parts.get(1)?.to_lowercase();
    let args = parts.get(2..).unwrap_or(&[]).to_vec();

    if has_option_tokens(&args) {
        return None;
    }

    let action = match flag.as_str() {
        "-s" | "--sync" => CanonicalAction::Install,
        "-r" | "--remove" => CanonicalAction::Remove,
        "-ss" => CanonicalAction::Search,
        "-sy" => CanonicalAction::Update,
        "-su" => CanonicalAction::Upgrade,
        "-syu" => CanonicalAction::UpdateUpgrade,
        _ => return None,
    };

    if matches!(
        action,
        CanonicalAction::Update | CanonicalAction::Upgrade | CanonicalAction::UpdateUpgrade
    ) && !args.is_empty()
    {
        return None;
    }

    if matches!(
        action,
        CanonicalAction::Install | CanonicalAction::Remove | CanonicalAction::Search
    ) && args.is_empty()
    {
        return None;
    }

    Some(ParsedCommand { action, args })
}

fn build_command(to: DistroFamily, parsed: ParsedCommand) -> Option<String> {
    match to {
        DistroFamily::Debian => build_debian_command(parsed),
        DistroFamily::Arch => build_arch_command(parsed),
    }
}

fn build_debian_command(parsed: ParsedCommand) -> Option<String> {
    let args = join_args(&parsed.args);
    let cmd = match parsed.action {
        CanonicalAction::Install => format!("apt install {}", args),
        CanonicalAction::Remove => format!("apt remove {}", args),
        CanonicalAction::Search => format!("apt search {}", args),
        CanonicalAction::Update => "apt update".to_string(),
        CanonicalAction::Upgrade => "apt upgrade".to_string(),
        CanonicalAction::UpdateUpgrade => "apt update && apt upgrade".to_string(),
    };

    Some(cmd)
}

fn build_arch_command(parsed: ParsedCommand) -> Option<String> {
    let args = join_args(&parsed.args);
    let cmd = match parsed.action {
        CanonicalAction::Install => format!("pacman -S {}", args),
        CanonicalAction::Remove => format!("pacman -R {}", args),
        CanonicalAction::Search => format!("pacman -Ss {}", args),
        CanonicalAction::Update => "pacman -Sy".to_string(),
        CanonicalAction::Upgrade => "pacman -Su".to_string(),
        CanonicalAction::UpdateUpgrade => "pacman -Syu".to_string(),
    };

    Some(cmd)
}

fn join_args(args: &[String]) -> String {
    args.join(" ")
}

fn has_option_tokens(args: &[String]) -> bool {
    args.iter().any(|arg| arg.starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_apt_install_to_pacman() {
        let parts = vec![
            "apt".to_string(),
            "install".to_string(),
            "neovim".to_string(),
        ];
        let converted = convert_parts_with_map(DistroFamily::Debian, DistroFamily::Arch, &parts);
        assert_eq!(converted, Some("pacman -S neovim".to_string()));
    }

    #[test]
    fn convert_pacman_install_to_apt() {
        let parts = vec!["pacman".to_string(), "-S".to_string(), "htop".to_string()];
        let converted = convert_parts_with_map(DistroFamily::Arch, DistroFamily::Debian, &parts);
        assert_eq!(converted, Some("apt install htop".to_string()));
    }

    #[test]
    fn convert_pacman_full_upgrade_to_apt() {
        let parts = vec!["pacman".to_string(), "-Syu".to_string()];
        let converted = convert_parts_with_map(DistroFamily::Arch, DistroFamily::Debian, &parts);
        assert_eq!(converted, Some("apt update && apt upgrade".to_string()));
    }
}
