use super::*;
use crate::config::SpeakConfig;
use crate::design::{WINDOW_MIN, WINDOW_SIZE};
use crate::gui::state::FIRST_RUN_STATUS;
use crate::gui::test_support::{Call, Recording, SERVER, clip, config, config_path, recording};
use common_gui_rune::test_harness::{
    FRAME, a11y_node, command, command_mods, element, has_key, has_text, press, press_key, rect,
    runtime_at, settle,
};
use rstest::rstest;
use rune::{Key, Runtime};
use serde_json::Value;
use std::rc::Rc;

const SPOKEN: &str = "Hello from the tests";
const SERVER_ISSUE_KEY: &str = "speak-server-issue";
const SETTINGS_ORDER: [&str; 8] = [
    AREAS_KEY,
    SERVER_KEY,
    ENDPOINT_KEY,
    METHOD_KEY,
    MODEL_KEY,
    VOICE_KEY,
    REACTIONS_BOX_KEY,
    SAVE_KEY,
];
const ISSUE_KEYS: [&str; 4] = [
    SERVER_ISSUE_KEY,
    "speak-model-issue",
    "speak-voice-issue",
    REACTIONS_ISSUE_KEY,
];

fn runtime_at_size(state: SpeakState, size: (f32, f32)) -> Runtime<SpeakState> {
    let mut rt = runtime_at(state, view, size);
    rt.on_key = Some(window_key);
    rt
}

fn window(config: Option<SpeakConfig>) -> (Runtime<SpeakState>, Rc<Recording>) {
    let (state, services) = recording(config);
    (runtime_at_size(state, WINDOW_SIZE), services)
}

fn deliver(rt: &mut Runtime<SpeakState>, input: Input) {
    rt.update_if(move |s, _| dispatch(s, input));
    settle(rt);
}

fn type_into(rt: &mut Runtime<SpeakState>, key: &str, text: &str) {
    press(rt, key);
    rt.paste(text);
    settle(rt);
}

fn focus_is_inside(rt: &Runtime<SpeakState>, key: &str) -> bool {
    let field = element(rt, key).path;
    rt.elements().into_iter().any(|element| {
        element.focused && (element.path == field || element.path.starts_with(&format!("{field}/")))
    })
}

fn focused_among(rt: &Runtime<SpeakState>, keys: &[&'static str]) -> Option<&'static str> {
    keys.iter()
        .copied()
        .find(|key| has_key(rt, key) && focus_is_inside(rt, key))
}

fn node(rt: &Runtime<SpeakState>, key: &str) -> Value {
    let tree = rt.accessibility_json();
    a11y_node(&tree, key)
        .cloned()
        .unwrap_or_else(|| panic!("no accessibility node keyed {key}"))
}

fn is_disabled(rt: &Runtime<SpeakState>, key: &str) -> bool {
    node(rt, key)["states"]
        .as_array()
        .is_some_and(|states| states.iter().any(|state| state == "disabled"))
}

fn bottom(rt: &Runtime<SpeakState>, key: &str) -> f32 {
    let [_, y, _, height] = rect(rt, key);
    y + height
}

fn many_reactions(count: usize) -> SpeakConfig {
    SpeakConfig {
        supported_vocal_reactions: (0..count).map(|index| format!("[sound {index}]")).collect(),
        ..config()
    }
}

#[test]
fn a_reaction_button_types_its_tag_at_the_caret_and_returns_focus_to_the_text() {
    let (mut rt, _) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, "Hello world");
    rt.update_if(|s, _| {
        s.text.set_caret("Hello".len());
        true
    });
    settle(&mut rt);

    press(&mut rt, &reaction_key(0));

    assert_eq!(rt.state.text.text, "Hello [laugh] world");
    assert_eq!(
        rt.state.text.range(),
        ("Hello [laugh]".len(), "Hello [laugh]".len())
    );
    assert!(focus_is_inside(&rt, TEXT_KEY));
}

#[test]
fn a_reaction_at_the_end_of_a_sentence_follows_one_space() {
    let (mut rt, _) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, "Hey, please pay attention!");

    press(&mut rt, &reaction_key(5));

    assert_eq!(
        rt.state.text.text,
        "Hey, please pay attention! [clear throat]"
    );
}

#[test]
fn a_first_run_shows_settings_with_focus_in_an_empty_server_and_the_defaults() {
    let (mut rt, _) = window(None);

    rt.update_if(shown);
    settle(&mut rt);

    assert!(has_key(&rt, SETTINGS_KEY));
    assert!(!has_key(&rt, COMPOSE_KEY));
    assert!(focus_is_inside(&rt, SERVER_KEY));
    assert_eq!(rt.state.form.server.text, "");
    assert_eq!(rt.state.form.endpoint.text, SPEECH_PATH);
    assert!(has_text(&rt, SPEECH_PATH));
    assert!(has_text(&rt, "chatterbox-turbo"));
    assert!(has_text(&rt, "sj-clip-001.wav"));
    assert!(has_text(&rt, "[clear throat]"));
}

#[test]
fn a_first_runs_compose_shows_the_default_reactions_and_cannot_generate() {
    let (mut rt, services) = window(None);
    command(&mut rt, Key::Command('1'));
    type_into(&mut rt, TEXT_KEY, SPOKEN);

    press(&mut rt, GENERATE_KEY);

    assert!(has_key(&rt, &reaction_key(8)));
    assert!(!has_key(&rt, &reaction_key(9)));
    assert!(is_disabled(&rt, GENERATE_KEY));
    assert_eq!(node(&rt, COMPOSE_STATUS_KEY)["name"], FIRST_RUN_STATUS);
    assert!(services.calls().is_empty());
}

#[test]
fn a_window_first_shown_with_a_config_moves_no_focus() {
    let (mut rt, _) = window(Some(config()));

    let changed = rt.update_if(shown);
    settle(&mut rt);

    assert!(!changed);
    assert!(rt.elements().iter().all(|element| !element.focused));
}

#[test]
fn generating_then_playing_goes_through_loading_ready_and_one_play() {
    let (mut rt, services) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);

    press(&mut rt, GENERATE_KEY);
    let while_generating = (
        rt.state.is_generating(),
        is_disabled(&rt, PLAY_KEY),
        node(&rt, COMPOSE_STATUS_KEY)["name"].clone(),
    );
    deliver(&mut rt, Input::Generated(Ok(clip())));
    let when_ready = (
        is_disabled(&rt, PLAY_KEY),
        node(&rt, COMPOSE_STATUS_KEY)["name"].clone(),
    );
    press(&mut rt, PLAY_KEY);

    assert_eq!(while_generating, (true, true, Value::from("Generating…")));
    assert_eq!(when_ready, (false, Value::from("Ready to play")));
    assert_eq!(
        services.calls(),
        vec![
            Call::Generate {
                config: config(),
                text: SPOKEN.to_owned(),
            },
            Call::Play(clip().size()),
        ]
    );
}

#[test]
fn a_second_press_of_generate_while_generating_asks_for_nothing_more() {
    let (mut rt, services) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);
    press(&mut rt, GENERATE_KEY);

    press(&mut rt, GENERATE_KEY);
    command(&mut rt, Key::Enter);

    assert_eq!(services.calls().len(), 1);
}

#[test]
fn a_long_failure_shows_cut_with_the_whole_text_as_its_description() {
    let (mut rt, _) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);
    press(&mut rt, GENERATE_KEY);
    let why = format!("The speech server answered 500: {}", "x".repeat(200));

    deliver(&mut rt, Input::Generated(Err(why.clone())));

    let status = node(&rt, COMPOSE_STATUS_KEY);
    let name = status["name"].as_str().unwrap_or_default();
    assert!(
        name.starts_with("Couldn't generate: The speech server"),
        "{name}"
    );
    assert_eq!(name.chars().count(), 141, "{name}");
    assert_eq!(status["description"], format!("Couldn't generate: {why}"));
    assert!(is_disabled(&rt, PLAY_KEY));
}

#[test]
fn a_play_failure_shows_in_the_status_line_and_play_stays_enabled() {
    let (mut rt, _) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);
    press(&mut rt, GENERATE_KEY);
    deliver(&mut rt, Input::Generated(Ok(clip())));

    deliver(
        &mut rt,
        Input::PlayFailed("No sound output device could be opened".to_owned()),
    );

    assert_eq!(
        node(&rt, COMPOSE_STATUS_KEY)["name"],
        "Couldn't play: No sound output device could be opened"
    );
    assert!(!is_disabled(&rt, PLAY_KEY));
}

#[test]
fn with_nothing_to_say_no_status_is_mounted_and_the_row_keeps_its_height() {
    let (mut rt, _) = window(Some(config()));
    let quiet = (
        has_key(&rt, COMPOSE_STATUS_KEY),
        rect(&rt, COMPOSE_ACTIONS_KEY)[3],
    );
    type_into(&mut rt, TEXT_KEY, SPOKEN);

    press(&mut rt, GENERATE_KEY);

    assert!(!quiet.0);
    assert_eq!(rect(&rt, COMPOSE_ACTIONS_KEY)[3], quiet.1);
}

#[test]
fn each_configured_reaction_has_a_button_labeled_with_its_tag_and_named_insert() {
    let reactions = SpeakConfig {
        supported_vocal_reactions: vec!["[hum]".to_owned(), "[clear throat]".to_owned()],
        ..config()
    };
    let (rt, _) = window(Some(reactions));

    let names = [
        node(&rt, &reaction_key(0))["name"].clone(),
        node(&rt, &reaction_key(1))["name"].clone(),
    ];

    assert_eq!(
        names,
        [
            Value::from("Insert hum"),
            Value::from("Insert clear throat")
        ]
    );
    assert!(has_text(&rt, "[hum]"));
    assert!(has_text(&rt, "[clear throat]"));
    assert!(!has_key(&rt, &reaction_key(2)));
    assert_eq!(node(&rt, REACTIONS_KEY)["name"], "Vocal reactions");
}

#[test]
fn a_long_tag_is_cut_on_its_button_and_no_wider_than_the_cap() {
    let long = format!("[{}]", "very long reaction ".repeat(4).trim_end());
    let reactions = SpeakConfig {
        supported_vocal_reactions: vec![long.clone()],
        ..config()
    };
    let (rt, _) = window(Some(reactions));

    assert!(has_text(&rt, "[very long reaction ve…"));
    assert!(!has_text(&rt, &long));
    assert!(rect(&rt, &reaction_key(0))[2] <= crate::design::REACTION_LABEL_MAX);
}

#[test]
fn a_save_with_a_changed_list_changes_the_buttons_compose_shows() {
    let (mut rt, services) = window(Some(config()));
    command(&mut rt, Key::Command('2'));
    press(&mut rt, REACTIONS_BOX_KEY);
    command(&mut rt, Key::Command('a'));
    rt.paste("[hum]");
    settle(&mut rt);
    press(&mut rt, SAVE_KEY);
    let Some(Call::Save { config: saved, .. }) = services.calls().pop() else {
        panic!("Save asked for no save");
    };

    deliver(&mut rt, Input::ConfigSaved(Ok(saved)));
    command(&mut rt, Key::Command('1'));

    assert_eq!(node(&rt, &reaction_key(0))["name"], "Insert hum");
    assert!(!has_key(&rt, &reaction_key(1)));
}

#[test]
fn an_empty_reaction_list_mounts_no_group() {
    let (rt, _) = window(Some(many_reactions(0)));

    assert!(!has_key(&rt, REACTIONS_KEY));
    assert!(has_key(&rt, TEXT_KEY));
}

#[rstest]
#[case::with_nothing_focused(None, None)]
#[case::from_the_text(Some(TEXT_KEY), None)]
#[case::from_the_server_field(None, Some(SERVER_KEY))]
#[case::from_the_endpoint_field(None, Some(ENDPOINT_KEY))]
#[case::from_the_reactions_box(None, Some(REACTIONS_BOX_KEY))]
fn mod_2_and_mod_1_switch_areas_whatever_has_focus(
    #[case] focus_on_compose: Option<&str>,
    #[case] focus_on_settings: Option<&str>,
) {
    let (mut rt, _) = window(Some(config()));
    if let Some(key) = focus_on_compose {
        press(&mut rt, key);
    }

    command(&mut rt, Key::Command('2'));
    let on_settings = has_key(&rt, SETTINGS_KEY) && !has_key(&rt, COMPOSE_KEY);
    if let Some(key) = focus_on_settings {
        press(&mut rt, key);
    }
    command(&mut rt, Key::Command('1'));

    assert!(on_settings);
    assert!(has_key(&rt, COMPOSE_KEY));
    assert!(!has_key(&rt, SETTINGS_KEY));
}

#[test]
fn the_area_that_enters_sits_where_it_stays_from_its_first_frame() {
    let (mut rt, _) = window(Some(config()));

    rt.key(KeyEvent {
        key: Key::Command('2'),
        mods: command_mods(),
    });
    rt.frame(FRAME);
    let entering = rect(&rt, SETTINGS_KEY);
    settle(&mut rt);

    assert_eq!(entering, rect(&rt, SETTINGS_KEY));
}

#[test]
fn the_area_switch_picks_an_area_on_a_press() {
    let (mut rt, _) = window(Some(config()));

    press(&mut rt, "segment1");

    assert_eq!(rt.state.area(), Area::Settings);
    assert!(has_key(&rt, SETTINGS_KEY));
}

#[rstest]
#[case::with_nothing_focused(false)]
#[case::from_the_text(true)]
fn mod_enter_on_compose_generates(#[case] typing: bool) {
    let (mut rt, services) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);
    if !typing {
        rt.update_if(|_, tx| {
            tx.blur();
            true
        });
        settle(&mut rt);
    }

    command(&mut rt, Key::Enter);

    assert_eq!(focus_is_inside(&rt, TEXT_KEY), typing);
    assert_eq!(services.calls().len(), 1);
    assert_eq!(rt.state.text.text, SPOKEN);
}

#[test]
fn mod_enter_on_settings_generates_nothing() {
    let (mut rt, services) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);
    command(&mut rt, Key::Command('2'));
    press(&mut rt, SERVER_KEY);

    command(&mut rt, Key::Enter);

    assert!(services.calls().is_empty());
}

#[test]
fn return_in_the_text_adds_a_line_and_generates_nothing() {
    let (mut rt, services) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, "one");

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.text.text, "one\n");
    assert!(services.calls().is_empty());
}

#[test]
fn tab_moves_through_the_settings_fields_and_then_save() {
    let (mut rt, _) = window(Some(config()));
    command(&mut rt, Key::Command('2'));
    let mut order = Vec::new();

    for _ in SETTINGS_ORDER {
        press_key(&mut rt, Key::Tab);
        order.push(focused_among(&rt, &SETTINGS_ORDER));
    }

    assert_eq!(order, SETTINGS_ORDER.map(Some));
}

#[test]
fn tab_on_compose_goes_through_the_switch_the_text_the_reactions_play_and_generate() {
    let (mut rt, _) = window(Some(config()));
    type_into(&mut rt, TEXT_KEY, SPOKEN);
    press(&mut rt, GENERATE_KEY);
    deliver(&mut rt, Input::Generated(Ok(clip())));
    rt.update_if(|_, tx| {
        tx.blur();
        true
    });
    settle(&mut rt);
    let stops = [AREAS_KEY, TEXT_KEY, REACTIONS_KEY, PLAY_KEY, GENERATE_KEY];
    let mut order = Vec::new();

    for _ in 0..13 {
        press_key(&mut rt, Key::Tab);
        order.push(focused_among(&rt, &stops));
    }
    order.dedup();

    assert_eq!(order, stops.map(Some));
}

#[test]
fn save_of_an_invalid_form_announces_each_issue_and_focuses_the_first_field() {
    let (mut rt, services) = window(None);
    rt.update_if(|s, _| {
        s.form.model = TextEdit::new("");
        s.form.voice = TextEdit::new("");
        s.form.reactions = TextEdit::new("laugh");
        true
    });
    settle(&mut rt);

    press(&mut rt, SAVE_KEY);

    let announced = ISSUE_KEYS.map(|key| {
        let issue = node(&rt, key);
        (issue["role"].clone(), issue["states"].clone())
    });
    assert_eq!(
        announced,
        ISSUE_KEYS.map(|_| (Value::from("status"), serde_json::json!(["live-polite"])))
    );
    assert_eq!(
        node(&rt, SERVER_ISSUE_KEY)["name"],
        "The speech server is not set"
    );
    assert!(focus_is_inside(&rt, SERVER_KEY));
    assert!(services.calls().is_empty());
}

#[test]
fn a_failed_save_with_a_bad_server_moves_focus_to_server_and_announces_its_issue() {
    let (mut rt, services) = window(Some(config()));
    command(&mut rt, Key::Command('2'));
    press(&mut rt, SERVER_KEY);
    command(&mut rt, Key::Command('a'));
    rt.paste("ftp://my-host");
    settle(&mut rt);
    press(&mut rt, MODEL_KEY);

    press(&mut rt, SAVE_KEY);

    let issue = node(&rt, SERVER_ISSUE_KEY);
    assert_eq!(
        issue["name"],
        "The speech server URL must start with http:// or https://"
    );
    assert_eq!(issue["role"], "status");
    assert!(focus_is_inside(&rt, SERVER_KEY));
    assert!(!has_key(&rt, "speak-endpoint-issue"));
    assert!(services.calls().is_empty());
}

#[test]
fn typing_in_endpoint_clears_the_server_issue() {
    let (mut rt, _) = window(None);
    press(&mut rt, SAVE_KEY);
    let had_issue = has_key(&rt, SERVER_ISSUE_KEY);

    type_into(&mut rt, ENDPOINT_KEY, "x");

    assert!(had_issue);
    assert!(!has_key(&rt, SERVER_ISSUE_KEY));
}

#[test]
fn typing_in_a_field_with_an_issue_clears_that_issue_only() {
    let (mut rt, _) = window(None);
    rt.update_if(|s, _| {
        s.form.model = TextEdit::new("");
        true
    });
    settle(&mut rt);
    press(&mut rt, SAVE_KEY);

    type_into(&mut rt, SERVER_KEY, SERVER);

    assert!(!has_key(&rt, SERVER_ISSUE_KEY));
    assert!(has_key(&rt, "speak-model-issue"));
}

#[test]
fn a_valid_save_asks_for_one_save_and_says_saved_when_it_lands() {
    let (mut rt, services) = window(None);
    type_into(&mut rt, SERVER_KEY, SERVER);

    press(&mut rt, SAVE_KEY);
    let while_saving = (rt.state.is_saving(), has_key(&rt, SAVE_STATUS_KEY));
    deliver(&mut rt, Input::ConfigSaved(Ok(config())));

    assert_eq!(
        services.calls(),
        vec![Call::Save {
            config: config(),
            path: config_path(),
        }]
    );
    assert_eq!(while_saving, (true, false));
    assert_eq!(node(&rt, SAVE_STATUS_KEY)["name"], "Saved");
}

#[test]
fn a_failed_save_says_why_and_an_edit_takes_the_words_away() {
    let (mut rt, _) = window(Some(config()));
    command(&mut rt, Key::Command('2'));
    press(&mut rt, SAVE_KEY);
    deliver(
        &mut rt,
        Input::ConfigSaved(Err("the disk is full".to_owned())),
    );
    let failed = node(&rt, SAVE_STATUS_KEY)["name"].clone();

    type_into(&mut rt, MODEL_KEY, "x");

    assert_eq!(failed, "Couldn't save: the disk is full");
    assert!(!has_key(&rt, SAVE_STATUS_KEY));
}

#[test]
fn return_in_the_reactions_box_adds_a_line_and_counts_as_an_edit() {
    let (mut rt, _) = window(Some(config()));
    command(&mut rt, Key::Command('2'));
    press(&mut rt, SAVE_KEY);
    deliver(&mut rt, Input::ConfigSaved(Ok(config())));
    press(&mut rt, REACTIONS_BOX_KEY);
    let lines = rt.state.form.reactions.text.lines().count();

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.form.reactions.text.matches('\n').count(), lines);
    assert!(!has_key(&rt, SAVE_STATUS_KEY));
}

#[test]
fn choosing_another_method_counts_as_an_edit() {
    let (mut rt, _) = window(Some(config()));
    command(&mut rt, Key::Command('2'));
    press(&mut rt, SAVE_KEY);
    deliver(&mut rt, Input::ConfigSaved(Ok(config())));

    press(&mut rt, METHOD_KEY);
    let open = rt.state.form.method_open;
    press(&mut rt, "option1");

    assert!(open);
    assert_eq!(rt.state.form.method, Method::Put);
    assert!(!rt.state.form.method_open);
    assert!(!has_key(&rt, SAVE_STATUS_KEY));
}

#[test]
fn the_settings_form_names_where_it_saves() {
    let (mut rt, _) = window(Some(config()));

    command(&mut rt, Key::Command('2'));

    assert!(has_text(&rt, &config_path().display().to_string()));
    assert!(has_text(&rt, "Saves to"));
}

#[test]
fn at_the_smallest_window_compose_keeps_every_control_inside_it() {
    let (state, _) = recording(Some(many_reactions(40)));
    let rt = runtime_at_size(state, WINDOW_MIN);

    let rows_cap = crate::design::REACTION_ROWS_MAX * CONTROL_HEIGHT
        + (crate::design::REACTION_ROWS_MAX - 1.0) * SPACE_SM;

    assert_eq!(rect(&rt, REACTIONS_SCROLL_KEY)[3], rows_cap);
    assert!(rect(&rt, TEXT_KEY)[3] >= TEXT_MIN_LINES * TYPE_BODY.line_px());
    assert!(bottom(&rt, REACTIONS_KEY) <= rect(&rt, COMPOSE_ACTIONS_KEY)[1]);
    assert_eq!(bottom(&rt, COMPOSE_ACTIONS_KEY), WINDOW_MIN.1);
    assert!(bottom(&rt, GENERATE_KEY) <= WINDOW_MIN.1);
    assert!(rect(&rt, PLAY_KEY)[0] >= 0.0);
}

#[test]
fn the_reaction_block_scrolls_to_the_buttons_past_its_rows() {
    let (state, _) = recording(Some(many_reactions(40)));
    let mut rt = runtime_at_size(state, WINDOW_MIN);
    let last = reaction_key(39);
    let before = rect(&rt, &last)[1];
    let [x, y, width, height] = rect(&rt, REACTIONS_SCROLL_KEY);

    rt.wheel(x + width / 2.0, y + height / 2.0, -120.0);
    settle(&mut rt);

    assert!(before > y + height);
    assert!(rect(&rt, &last)[1] < before);
}

#[test]
fn at_the_smallest_window_settings_keeps_save_inside_and_scrolls_the_form() {
    let (state, _) = recording(None);
    let mut rt = runtime_at_size(state, WINDOW_MIN);
    let before = rect(&rt, SERVER_KEY)[1];
    let [x, y, width, height] = rect(&rt, SETTINGS_SCROLL_KEY);

    rt.wheel(x + width / 2.0, y + 8.0, -120.0);
    settle(&mut rt);

    assert_eq!(bottom(&rt, SETTINGS_FOOTER_KEY), WINDOW_MIN.1);
    assert!(bottom(&rt, SAVE_KEY) <= WINDOW_MIN.1);
    assert!(y + height <= rect(&rt, SETTINGS_FOOTER_KEY)[1]);
    assert!(rect(&rt, SERVER_KEY)[1] < before);
}

#[test]
fn a_loaded_config_opens_on_compose_with_settings_filled_from_it() {
    let (mut rt, _) = window(Some(config()));
    let opened_on_compose = has_key(&rt, COMPOSE_KEY) && !has_key(&rt, SETTINGS_KEY);

    command(&mut rt, Key::Command('2'));

    assert!(opened_on_compose);
    assert!(has_text(&rt, SERVER));
    assert!(has_text(&rt, SPEECH_PATH));
    assert!(has_text(&rt, "chatterbox-turbo"));
}

#[test]
fn the_window_draws_in_the_appearance_it_is_told() {
    let (mut rt, _) = window(Some(config()));

    let first = rt.update_if(|s, _| set_appearance(s, false));
    let again = rt.update_if(|s, _| set_appearance(s, false));

    assert!(first);
    assert!(!again);
}

#[rstest]
#[case::server(Field::Server, SERVER_KEY)]
#[case::endpoint(Field::Endpoint, ENDPOINT_KEY)]
#[case::method(Field::Method, METHOD_KEY)]
#[case::model(Field::Model, MODEL_KEY)]
#[case::voice(Field::Voice, VOICE_KEY)]
#[case::reactions(Field::Reactions, REACTIONS_BOX_KEY)]
fn each_field_has_the_key_of_its_control(#[case] field: Field, #[case] key: &str) {
    let (mut rt, _) = window(Some(config()));
    command(&mut rt, Key::Command('2'));

    assert_eq!(field_key(field), key);
    assert!(has_key(&rt, key));
}
