pub fn sanitize_string_for_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            // Replace filesystem-unsafe characters with underscores
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\' | '/' => '_',
            // Replace spaces with hyphens for better readability
            ' ' => '-',
            // Keep alphanumeric, dots, hyphens, and underscores
            c if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' => c,
            // Replace any other special characters with underscores
            _ => '_',
        })
        .collect::<String>()
        // Ensure the filename doesn't start or end with dots (problematic on some systems)
        .trim_matches('.')
        .to_string()
}
