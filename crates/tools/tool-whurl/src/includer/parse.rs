use once_cell::sync::OnceCell;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct IncludeDirective {
    pub path: String,
    pub options: Vec<String>,
    pub feed: Vec<FeedAssignment>,
    pub line_number: u32,
}

/// One `KEY=VALUE` entry of an include's feed clause
/// (`# @include path -> { key="value", other={{name}} }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedAssignment {
    pub key: String,
    pub value: FeedValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedValue {
    Literal(String),
    Reference(String),
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

/// A malformed directive in a file's top comment block; the caller adds the
/// file path.
#[derive(Debug)]
pub struct DirectiveParseError {
    pub line: u32,
    pub message: String,
}

pub fn parse_top_comment_directives(contents: &str) -> Result<FileDirectives, DirectiveParseError> {
    static INCLUDE_RE: OnceCell<Regex> = OnceCell::new();
    static VARS_RE: OnceCell<Regex> = OnceCell::new();

    // The feed capture is greedy to the LAST closing brace on the line, so
    // `{{reference}}` values stay inside it; shape validation happens in
    // parse_feed.
    let include_re = INCLUDE_RE.get_or_init(|| {
        Regex::new(
            r"(?i)^#\s*@include(?:\s*:\s*\[(?P<opts>[^\]]*)\])?\s+(?P<path>.+?)(?:\s*->\s*\{(?P<feed>.*)\})?\s*$",
        )
        .expect("invalid include regex")
    });

    let vars_re = VARS_RE.get_or_init(|| {
        Regex::new(r"(?i)^#\s*@vars\s+(?P<name>.+?)\s*$").expect("invalid vars regex")
    });

    let mut directives = FileDirectives::default();

    for (idx, line) in contents.lines().enumerate() {
        let line_number = idx as u32 + 1;
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

                let path = path.as_str().trim().to_string();

                let feed = match caps.name("feed") {
                    Some(feed) => parse_feed(feed.as_str(), line_number)?,
                    None if path.contains("->") => {
                        // The arrow clause failed to match, so its text got
                        // swallowed into the path (trailing content after the
                        // closing brace, a lone arrow, or a literal `->` in a
                        // path, which is unsupported).
                        return Err(DirectiveParseError {
                            line: line_number,
                            message: "malformed feed clause; expected `-> { key=value, ... }` \
                                      with nothing after the closing brace"
                                .to_string(),
                        });
                    }
                    None => Vec::new(),
                };

                directives.includes.push(IncludeDirective {
                    path,
                    options,
                    feed,
                    line_number,
                });
                continue;
            }
        }

        if let Some(caps) = vars_re.captures(trimmed) {
            if let Some(name) = caps.name("name") {
                directives.vars.push(VarsDirective {
                    name: name.as_str().trim().to_string(),
                    _line_number: line_number,
                });
            }
        }
    }

    Ok(directives)
}

fn parse_feed(inner: &str, line: u32) -> Result<Vec<FeedAssignment>, DirectiveParseError> {
    let error = |message: String| DirectiveParseError { line, message };

    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Comma split, ignoring commas inside double quotes.
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in inner.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => items.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    if in_quotes {
        return Err(error("unterminated quote in feed value".to_string()));
    }
    items.push(current);

    let mut assignments = Vec::with_capacity(items.len());
    for item in items {
        let item = item.trim();
        if item.is_empty() {
            return Err(error("empty feed entry".to_string()));
        }

        let Some((raw_key, raw_value)) = item.split_once('=') else {
            return Err(error(
                "feed entry is missing `=`; expected KEY=VALUE".to_string(),
            ));
        };

        let key = raw_key.trim();
        if key.is_empty() {
            return Err(error("feed key cannot be empty".to_string()));
        }
        if key.contains(char::is_whitespace) {
            return Err(error(format!("feed key `{key}` cannot contain whitespace")));
        }

        let value = parse_feed_value(key, raw_value.trim()).map_err(error)?;
        assignments.push(FeedAssignment {
            key: key.to_string(),
            value,
        });
    }

    Ok(assignments)
}

fn parse_feed_value(key: &str, raw: &str) -> Result<FeedValue, String> {
    if raw.is_empty() {
        return Err(format!("feed value for key `{key}` cannot be empty"));
    }

    if let Some(quoted) = raw.strip_prefix('"') {
        let Some(literal) = quoted.strip_suffix('"') else {
            return Err(format!(
                "feed value for key `{key}` has an unterminated quote"
            ));
        };
        if literal.contains('"') {
            return Err(format!(
                "feed value for key `{key}` contains a stray quote; escapes are not supported"
            ));
        }
        return Ok(FeedValue::Literal(literal.to_string()));
    }

    if let Some(reference) = raw
        .strip_prefix("{{")
        .and_then(|rest| rest.strip_suffix("}}"))
    {
        let name = reference.trim();
        if name.is_empty() {
            return Err(format!("feed reference for key `{key}` names no variable"));
        }
        if name.contains(char::is_whitespace) || name.contains(['{', '}']) {
            return Err(format!(
                "feed reference for key `{key}` has an invalid variable name"
            ));
        }
        return Ok(FeedValue::Reference(name.to_string()));
    }

    // A bare literal must be a single unquoted word; anything shaped like a
    // half-quoted or half-braced value fails loudly instead of feeding junk.
    if raw.contains(char::is_whitespace) || raw.contains(['"', '{', '}', ',']) {
        return Err(format!(
            "invalid feed value for key `{key}`; use a bare word, a double-quoted literal, or a \
             {{{{reference}}}}"
        ));
    }

    Ok(FeedValue::Literal(raw.to_string()))
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
        let directives = parse_top_comment_directives(contents).expect("parse");
        assert_eq!(directives.includes.len(), 1);
        assert_eq!(directives.includes[0].path, "helper/login");
        assert!(directives.includes[0].feed.is_empty());
        assert_eq!(directives.vars.len(), 1);
        assert_eq!(directives.vars[0].name, "session");
    }

    #[test]
    fn include_options_populate_and_parsing_stops_at_first_non_comment_line() {
        let contents = "\
# @include:[quiet, silent] helpers/login
# @vars session
GET https://example.com
# @include ignored
# @vars ignored
";
        let directives = parse_top_comment_directives(contents).expect("parse");
        assert_eq!(directives.includes.len(), 1);
        assert_eq!(directives.includes[0].path, "helpers/login");
        assert_eq!(
            directives.includes[0].options,
            vec!["quiet".to_string(), "silent".to_string()]
        );
        assert_eq!(directives.includes[0].line_number, 1);
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
        let directives = parse_top_comment_directives(contents).expect("parse");
        assert_eq!(directives.includes.len(), 1);
        assert!(directives.vars.is_empty());
    }

    #[test]
    fn parses_the_feed_clause_with_literals_numbers_and_references() {
        let contents = "# @include my-get-request -> { queryStringType=\"question\", correctAnswer=42, extra={{user}} }\n";
        let directives = parse_top_comment_directives(contents).expect("parse");

        assert_eq!(directives.includes.len(), 1);
        let include = &directives.includes[0];
        assert_eq!(include.path, "my-get-request");
        assert_eq!(
            include.feed,
            vec![
                FeedAssignment {
                    key: "queryStringType".to_string(),
                    value: FeedValue::Literal("question".to_string()),
                },
                FeedAssignment {
                    key: "correctAnswer".to_string(),
                    value: FeedValue::Literal("42".to_string()),
                },
                FeedAssignment {
                    key: "extra".to_string(),
                    value: FeedValue::Reference("user".to_string()),
                },
            ]
        );
    }

    #[test]
    fn feed_quoted_literals_keep_commas_and_paths_keep_spaces() {
        let contents = "# @include:[quiet] dog facts/breeds -> { s=\"a, b\", n=42 }\n";
        let directives = parse_top_comment_directives(contents).expect("parse");

        let include = &directives.includes[0];
        assert_eq!(include.path, "dog facts/breeds");
        assert_eq!(include.options, vec!["quiet".to_string()]);
        assert_eq!(
            include.feed,
            vec![
                FeedAssignment {
                    key: "s".to_string(),
                    value: FeedValue::Literal("a, b".to_string()),
                },
                FeedAssignment {
                    key: "n".to_string(),
                    value: FeedValue::Literal("42".to_string()),
                },
            ]
        );
    }

    #[test]
    fn empty_feed_clause_is_a_no_op() {
        let directives = parse_top_comment_directives("# @include basic -> {}\n").expect("parse");
        assert_eq!(directives.includes.len(), 1);
        assert!(directives.includes[0].feed.is_empty());
    }

    #[test]
    fn malformed_feeds_error_with_the_line_number() {
        let cases = [
            ("# @include x -> { a }\n", "missing `=`"),
            ("# @include x -> { =1 }\n", "key cannot be empty"),
            ("# @include x -> { a=\"b }\n", "unterminated quote"),
            ("# @include x -> { a=b c }\n", "invalid feed value"),
            ("# @include x -> { a={{ }} }\n", "names no variable"),
            ("# @include x -> { a=1,, b=2 }\n", "empty feed entry"),
            ("# @include x -> { a=1 } junk\n", "malformed feed clause"),
            ("# @include x ->\n", "malformed feed clause"),
        ];

        for (contents, expected) in cases {
            let err = parse_top_comment_directives(contents).expect_err("malformed feed must fail");
            assert_eq!(err.line, 1, "line for {contents:?}");
            assert!(
                err.message.contains(expected),
                "message {:?} should contain {:?}",
                err.message,
                expected
            );
        }
    }

    #[test]
    fn a_path_ending_in_a_brace_without_an_arrow_stays_a_path() {
        let directives = parse_top_comment_directives("# @include weird}\n").expect("parse");
        assert_eq!(directives.includes[0].path, "weird}");
        assert!(directives.includes[0].feed.is_empty());
    }
}
