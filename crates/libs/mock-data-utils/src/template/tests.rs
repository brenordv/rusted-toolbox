use super::*;
use crate::MAX_LENGTH;
use rstest::rstest;

const CAP: usize = 1 << 20;

fn compile_error(source: &str) -> String {
    format!("{:#}", Template::compile(source).unwrap_err())
}

#[rstest]
#[case::plain("plain text", "plain text")]
#[case::escaped("a \\{{ b", "a {{ b")]
#[case::closing_braces_alone("a }} b", "a }} b")]
#[case::other_backslashes("logs\\day \\n \\{", "logs\\day \\n \\{")]
#[case::lone_brace("{\"a\": 1}", "{\"a\": 1}")]
#[case::backslash_before_a_placeholder(
    "logs\\\\{{internet.username}}",
    "logs\\{{internet.username}}"
)]
#[case::empty("", "")]
fn text_without_placeholders_renders_as_written(#[case] source: &str, #[case] rendered: &str) {
    let template = Template::compile(source).unwrap();

    assert_eq!(template.render(CAP).unwrap(), rendered);
    assert_eq!(template.placeholder_count(), 0);
}

#[test]
fn every_data_type_compiles_and_renders_as_a_placeholder() {
    for data_type in &DataType::ALL {
        let source = format!("{{{{{}}}}}", data_type.command());

        let template = Template::compile(&source).unwrap();

        assert_eq!(template.placeholder_count(), 1, "{source}");
        assert!(template.render(CAP).is_ok(), "{source}");
    }
}

#[test]
fn a_fixed_integer_renders_its_value() {
    let template = Template::compile("{{random.integer min=5 max=5}}").unwrap();

    assert_eq!(template.render(CAP).unwrap(), "5");
}

#[test]
fn an_integer_range_stays_inside_its_bounds() {
    let template = Template::compile("{{random.integer min=1 max=6}}").unwrap();

    for _ in 0..200 {
        let value: i32 = template.render(CAP).unwrap().parse().unwrap();
        assert!((1..=6).contains(&value), "{value}");
    }
}

#[rstest]
#[case::unclosed(
    "{\"a\": 1,\n\"b\": 2,\n  \"c\": {{person.email",
    "line 3, column 8: '{{' is not closed"
)]
#[case::empty("x {{  }}", "line 1, column 3: the placeholder is empty")]
#[case::unknown_type(
    "{{person.fullname}}",
    "line 1, column 1: unknown type 'person.fullname'"
)]
#[case::unknown_option(
    "{{person.email colour=red}}",
    "line 1, column 1: unknown option 'colour' for person.email"
)]
#[case::bad_number(
    "{{random.integer min=x}}",
    "line 1, column 1: 'min' needs a whole number, not 'x'"
)]
#[case::negative_unsigned(
    "{{random.date range=-1}}",
    "line 1, column 1: 'range' needs a whole number, not '-1'"
)]
#[case::missing_value("{{random.integer min}}", "line 1, column 1: 'min' needs a value")]
#[case::bare_word_with_value("{{random.date past=1}}", "line 1, column 1: 'past' takes no value")]
#[case::twice(
    "{{random.integer min=1 min=2}}",
    "line 1, column 1: 'min' is given twice"
)]
#[case::min_over_max(
    "{{random.integer min=9 max=1}}",
    "line 1, column 1: min (9) must be <= max (1)"
)]
#[case::range_zero("{{random.date range=0}}", "line 1, column 1: range must be >= 1")]
#[case::length_over_cap(
    "{{internet.password length=1048577}}",
    "line 1, column 1: length (1048577) must be <= 1048576"
)]
#[case::unknown_locale(
    "{{person.city locale=xx-yy}}",
    "line 1, column 1: unknown locale 'xx-yy'; the codes are en,"
)]
fn compile_errors_name_the_line_column_and_problem(#[case] source: &str, #[case] expected: &str) {
    let error = compile_error(source);

    assert!(error.starts_with(expected), "{error}");
}

#[test]
fn the_length_cap_itself_compiles() {
    assert!(Template::compile(&format!("{{{{internet.password length={MAX_LENGTH}}}}}")).is_ok());
}

#[test]
fn a_long_unknown_type_is_quoted_cut_short() {
    let name = "x".repeat(200);

    let error = compile_error(&format!("{{{{{name}}}}}"));

    assert!(
        error.ends_with(&format!("unknown type '{}…'", "x".repeat(QUOTE_LIMIT))),
        "{error}"
    );
}

#[test]
fn columns_count_characters_after_non_ascii_text() {
    let error = compile_error("café ☕ {{nope}}");

    assert!(error.starts_with("line 1, column 8:"), "{error}");
}

#[test]
fn positions_follow_earlier_placeholders_and_escapes() {
    let error = compile_error("{{random.integer}} \\{{ x\n  {{bad}}");

    assert!(error.starts_with("line 2, column 3:"), "{error}");
}

#[test]
fn a_non_default_locale_on_an_unaware_type_gives_one_note() {
    let template = Template::compile("{{random.integer locale=pt-br}}").unwrap();

    assert_eq!(
        template.inert_locale_notes(),
        [
            "line 1, column 1: locale=pt-br has no effect on random.integer, which is not locale-aware"
        ]
    );
}

#[test]
fn a_repeated_inert_locale_gives_one_note_per_type_and_locale() {
    let template = Template::compile(
        "{{random.integer locale=pt-br}} {{random.integer locale=pt-br}} \
         {{random.integer locale=ja-jp}} {{random.float locale=pt-br}}",
    )
    .unwrap();

    assert_eq!(template.inert_locale_notes().len(), 3);
}

#[test]
fn a_template_past_the_placeholder_cap_is_refused_and_the_cap_itself_compiles() {
    let at_cap = "{{random.integer}}".repeat(MAX_PLACEHOLDERS);
    let over_cap = format!("{at_cap}{{{{random.integer}}}}");

    let compiled = Template::compile(&at_cap).map(|template| template.placeholder_count());
    let error = compile_error(&over_cap);

    assert_eq!(compiled.unwrap(), MAX_PLACEHOLDERS);
    assert!(
        error.ends_with(&format!("has more than {MAX_PLACEHOLDERS} placeholders")),
        "{error}"
    );
}

#[test]
fn a_long_non_ascii_option_value_is_quoted_cut_at_a_character() {
    let value = "é".repeat(60);

    let error = compile_error(&format!("{{{{random.integer min={value}}}}}"));

    assert!(
        error.ends_with(&format!("not '{}…'", "é".repeat(QUOTE_LIMIT))),
        "{error}"
    );
}

#[test]
fn a_render_error_holds_none_of_the_templates_text() {
    let template = Template::compile(
        "secret-literal {{internet.password\n   length=600000  }} {{internet.password length=600000}}",
    )
    .unwrap();

    let error = format!("{:#}", template.render(CAP).unwrap_err());

    assert!(!error.contains("secret-literal"), "{error}");
    assert!(!error.contains("length="), "{error}");
}

#[test]
fn a_locale_on_an_aware_type_gives_no_note() {
    let template = Template::compile("{{person.city locale=pt-br}}").unwrap();

    assert!(template.inert_locale_notes().is_empty());
}

#[test]
fn whitespace_and_newlines_inside_braces_are_ignored_and_text_around_is_kept() {
    let template = Template::compile("Olá «{{\n  random.integer\tmin=3   max=3\n}}» ☕\n").unwrap();

    assert_eq!(template.render(CAP).unwrap(), "Olá «3» ☕\n");
}

#[test]
fn render_fails_past_the_cap_naming_the_placeholder_that_crossed_it() {
    let template =
        Template::compile("{{internet.password length=600000}}{{internet.password length=600000}}")
            .unwrap();

    let error = format!("{:#}", template.render(CAP).unwrap_err());

    assert_eq!(
        error,
        "line 1, column 36: internet.password: the rendered body passes the 1048576-byte limit"
    );
}

#[test]
fn render_under_the_cap_succeeds() {
    let template = Template::compile("{{internet.password length=600000}}").unwrap();

    assert_eq!(template.render(CAP).unwrap().len(), 600_000);
}

#[test]
fn render_fails_when_literal_text_passes_the_cap() {
    let template = Template::compile("0123456789").unwrap();

    let error = format!("{:#}", template.render(4).unwrap_err());

    assert_eq!(error, "the rendered body passes the 4-byte limit");
}

#[test]
fn two_renders_draw_fresh_values() {
    let template = Template::compile("{{internet.password length=64}}").unwrap();

    assert_ne!(template.render(CAP).unwrap(), template.render(CAP).unwrap());
}

#[test]
fn debug_shows_counts_and_none_of_the_text() {
    let template = Template::compile("secret-body {{person.email}} tail").unwrap();

    let rendered = format!("{template:?}");

    assert_eq!(rendered, "Template { segments: 3, placeholders: 1 }");
}

#[test]
fn placeholder_groups_cover_every_type_once_under_its_prefix() {
    let groups = placeholder_groups();

    let prefixes: Vec<&str> = groups.iter().map(|(prefix, _)| *prefix).collect();
    let total: usize = groups.iter().map(|(_, commands)| commands.len()).sum();
    assert_eq!(prefixes, ["person", "internet", "random", "commerce"]);
    assert_eq!(total, DataType::ALL.len());
    assert!(groups[0].1.contains(&"person.first-name"));
}
