use once_cell::sync::OnceCell;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct IncludeDirective {
    pub path: String,
    pub options: Vec<String>,
    pub line_number: u32,
}

pub fn parse_top_comment_includes(contents: &str) -> Vec<IncludeDirective> {
    static INCLUDE_RE: OnceCell<Regex> = OnceCell::new();
    let include_re = INCLUDE_RE.get_or_init(|| {
        Regex::new(r"^#\s*@include(?:\s*:\s*\[(?P<opts>[^\]]*)\])?\s+(?P<path>.+?)\s*$")
            .expect("invalid include regex")
    });

    let mut directives = Vec::new();

    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim_end();
        if !trimmed.starts_with('#') {
            break;
        }

        if let Some(caps) = include_re.captures(trimmed) {
            if let Some(path) = caps.name("path") {
                let options = caps
                    .name("opts")
                    .map(|m| {
                        m.as_str()
                            .split(',')
                            .filter_map(|opt| {
                                let trimmed = opt.trim();
                                if trimmed.is_empty() {
                                    None
                                } else {
                                    Some(trimmed.to_string())
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                directives.push(IncludeDirective {
                    path: path.as_str().trim().to_string(),
                    options,
                    line_number: idx as u32 + 1,
                });
            }
        }
    }

    directives
}
