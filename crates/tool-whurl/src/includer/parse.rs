use once_cell::sync::OnceCell;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct IncludeDirective {
    pub path: String,
    pub options: Vec<String>,
    pub line_number: u32,
}

#[derive(Debug, Clone)]
pub struct VarsDirective {
    pub name: String,
    pub _line_number: u32,
}

#[derive(Debug, Default)]
pub struct FileDirectives {
    pub includes: Vec<IncludeDirective>,
    pub vars: Vec<VarsDirective>,
}

pub fn parse_top_comment_directives(contents: &str) -> FileDirectives {
    static INCLUDE_RE: OnceCell<Regex> = OnceCell::new();
    static VARS_RE: OnceCell<Regex> = OnceCell::new();

    let include_re = INCLUDE_RE.get_or_init(|| {
        Regex::new(r"(?i)^#\s*@include(?:\s*:\s*\[(?P<opts>[^\]]*)\])?\s+(?P<path>.+?)\s*$")
            .expect("invalid include regex")
    });

    let vars_re = VARS_RE.get_or_init(|| {
        Regex::new(r"(?i)^#\s*@vars\s+(?P<name>.+?)\s*$").expect("invalid vars regex")
    });

    let mut directives = FileDirectives::default();

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

                directives.includes.push(IncludeDirective {
                    path: path.as_str().trim().to_string(),
                    options,
                    line_number: idx as u32 + 1,
                });
                continue;
            }
        }

        if let Some(caps) = vars_re.captures(trimmed) {
            if let Some(name) = caps.name("name") {
                directives.vars.push(VarsDirective {
                    name: name.as_str().trim().to_string(),
                    _line_number: idx as u32 + 1,
                });
            }
        }
    }

    directives
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_include_and_vars_directives() {
        let contents = "\
# @Include helper/login
# @vars session

GET https://example.com
";
        let directives = parse_top_comment_directives(contents);
        assert_eq!(directives.includes.len(), 1);
        assert_eq!(directives.includes[0].path, "helper/login");
        assert_eq!(directives.vars.len(), 1);
        assert_eq!(directives.vars[0].name, "session");
    }

    #[test]
    fn ignores_non_comment_lines() {
        let contents = "\
# @include alpha
GET https://example.com
# @vars beta
";
        let directives = parse_top_comment_directives(contents);
        assert_eq!(directives.includes.len(), 1);
        assert!(directives.vars.is_empty());
    }
}
