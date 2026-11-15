use super::*;

fn ready(body: &str) -> TemplateState {
    TemplateState::for_body(true, body)
}

fn generation(state: &TemplateState) -> Option<u64> {
    match state {
        TemplateState::Ready { generation, .. } => Some(*generation),
        TemplateState::Off | TemplateState::OverCap | TemplateState::Invalid(_) => None,
    }
}

#[test]
fn off_compiles_nothing_whatever_the_body() {
    let state = TemplateState::for_body(false, "{{person.fullname}}");

    assert!(matches!(state, TemplateState::Off));
    assert!(state.issue().is_none() && state.template().is_none());
}

#[test]
fn a_body_that_compiles_is_ready_with_a_first_sample() {
    let state = ready("n={{random.integer min=3 max=3}}");

    let TemplateState::Ready {
        template,
        sample,
        generation,
    } = &state
    else {
        panic!("expected Ready, got {state:?}");
    };
    assert_eq!(template.placeholder_count(), 1);
    assert_eq!(sample.as_deref(), Ok("n=3"));
    assert_eq!(*generation, 0);
}

#[test]
fn a_body_that_does_not_compile_is_invalid_with_a_capitalized_position() {
    let state = ready("{{person.fullname}}");

    assert_eq!(
        state.issue(),
        Some("Line 1, column 1: unknown type 'person.fullname'")
    );
    assert!(state.template().is_none());
}

#[test]
fn a_body_over_the_cap_is_not_compiled() {
    let body = format!("{{{{person.fullname}}}}{}", "x".repeat(BODY_CAP));

    let state = ready(&body);

    assert!(matches!(state, TemplateState::OverCap), "{state:?}");
    assert!(state.issue().is_none());
}

#[test]
fn a_render_that_fails_is_kept_as_the_samples_error() {
    let state = ready("{{internet.password length=1048576}}x");

    let TemplateState::Ready { sample, .. } = &state else {
        panic!("expected Ready, got {state:?}");
    };
    let error = sample.as_ref().unwrap_err();
    assert!(error.contains("the rendered body passes the"), "{error}");
    assert!(state.issue().is_none());
}

#[test]
fn regenerate_renders_again_only_when_ready() {
    let mut ready = ready("{{internet.password}}");
    let mut invalid = TemplateState::for_body(true, "{{nope}}");
    let mut off = TemplateState::Off;

    let renders = [ready.regenerate(), invalid.regenerate(), off.regenerate()];

    assert_eq!(renders, [true, false, false]);
    assert_eq!(generation(&ready), Some(1));
}

#[test]
fn a_short_sample_is_shown_whole() {
    assert_eq!(shown_sample("{\"a\": 1}"), ("{\"a\": 1}", false));
}

#[test]
fn a_long_sample_is_cut_at_a_character_boundary_below_the_limit() {
    let sample = format!("{}é tail", "x".repeat(SAMPLE_SHOWN_BYTES - 1));

    let (shown, cut) = shown_sample(&sample);

    assert!(cut);
    assert_eq!(shown.len(), SAMPLE_SHOWN_BYTES - 1);
    assert!(shown.chars().all(|c| c == 'x'));
}

#[test]
fn debug_shows_the_state_and_counts_but_no_text() {
    let ready = ready("secret-body {{person.email}}");
    let invalid = TemplateState::for_body(true, "{{secret-type}}");

    let rendered = format!("{ready:?} {invalid:?}");

    assert!(!rendered.contains("secret"), "{rendered}");
    assert!(rendered.contains("placeholders: 1"), "{rendered}");
}
