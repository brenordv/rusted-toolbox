use ai_macros::ai_function;

#[ai_function]
fn increment(value: i32) -> i32 {
    value + 7
}

#[ai_function]
pub fn shout(text: &str) -> String {
    text.to_uppercase()
}

#[test]
fn original_function_remains_callable() {
    assert_eq!(increment(35), 42);
    assert_eq!(shout("hi"), "HI");
}

#[test]
fn generated_helper_returns_function_source() {
    let source = increment_as_string();

    assert!(source.contains("increment"), "source was: {source}");
    assert!(source.contains("fn"), "source was: {source}");
    assert!(source.contains("i32"), "source was: {source}");
}

#[test]
fn generated_helper_includes_function_body() {
    let source = increment_as_string();

    assert!(source.contains('7'), "source was: {source}");
}

#[test]
fn generated_helper_exposed_with_inherited_visibility() {
    let source = shout_as_string();

    assert!(source.contains("shout"), "source was: {source}");
    assert!(source.contains("to_uppercase"), "source was: {source}");
}
