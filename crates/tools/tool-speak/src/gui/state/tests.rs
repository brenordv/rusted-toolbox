use super::*;
use crate::config::SPEECH_PATH;
use crate::gui::test_support::{SERVER, clip, config, config_path};
use common_audio::test_support::tone_wav;
use rstest::rstest;
use serde_json::json;

const SPOKEN: &str = "Hello from the tests";
const WHY: &str = "The speech server at my-host:10157 answered 500 Internal Server Error";
const PLAY_WHY: &str = "No sound output device could be opened";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Audio {
    Empty,
    Generating,
    Ready,
    Failed,
}

fn audio_of(s: &SpeakState) -> Audio {
    match s.audio() {
        ComposeAudio::Empty => Audio::Empty,
        ComposeAudio::Generating => Audio::Generating,
        ComposeAudio::Ready { .. } => Audio::Ready,
        ComposeAudio::Failed(_) => Audio::Failed,
    }
}

fn another_clip() -> Clip {
    let mut bytes = tone_wav();
    bytes.extend_from_slice(&[0, 0]);
    Clip::from_bytes(bytes).unwrap()
}

fn first_run() -> SpeakState {
    SpeakState::new(None, config_path(), None)
}

fn configured() -> SpeakState {
    let mut s = SpeakState::new(Some(config()), config_path(), None);
    s.text = TextEdit::new(SPOKEN);
    s
}

fn composing(audio: Audio) -> SpeakState {
    let mut s = configured();
    s.audio = match audio {
        Audio::Empty => ComposeAudio::Empty,
        Audio::Generating => ComposeAudio::Generating,
        Audio::Ready => ComposeAudio::Ready {
            clip: clip(),
            play_issue: None,
        },
        Audio::Failed => ComposeAudio::Failed(WHY.to_owned()),
    };
    s
}

fn saving(save: SaveStatus) -> SpeakState {
    let mut s = configured();
    s.save = save;
    s
}

fn play_issue(s: &SpeakState) -> Option<&str> {
    match s.audio() {
        ComposeAudio::Ready { play_issue, .. } => play_issue.as_deref(),
        ComposeAudio::Empty | ComposeAudio::Generating | ComposeAudio::Failed(_) => None,
    }
}

fn ready_clip_size(s: &SpeakState) -> Option<usize> {
    match s.audio() {
        ComposeAudio::Ready { clip, .. } => Some(clip.size()),
        ComposeAudio::Empty | ComposeAudio::Generating | ComposeAudio::Failed(_) => None,
    }
}

#[rstest]
#[case::from_empty(Audio::Empty)]
#[case::from_ready(Audio::Ready)]
#[case::from_failed(Audio::Failed)]
fn generate_with_text_and_a_config_starts_one_request_carrying_them(#[case] from: Audio) {
    let mut s = composing(from);
    s.text = TextEdit::new(format!("  {SPOKEN}\n"));

    let outcome = s.apply(Input::GeneratePressed);

    assert_eq!(audio_of(&s), Audio::Generating);
    assert!(outcome.changed);
    assert!(
        matches!(
            outcome.effects.as_slice(),
            [Effect::Generate { config: sent, text }] if *sent == config() && text == SPOKEN
        ),
        "{:?}",
        outcome.effects
    );
}

#[rstest]
#[case::from_empty(Audio::Empty)]
#[case::from_ready(Audio::Ready)]
#[case::from_failed(Audio::Failed)]
fn generate_with_blank_text_does_nothing(#[case] from: Audio) {
    let mut s = composing(from);
    s.text = TextEdit::new("  \n ");

    let outcome = s.apply(Input::GeneratePressed);

    assert_eq!(audio_of(&s), from);
    assert!(!outcome.changed);
    assert!(outcome.effects.is_empty());
}

#[rstest]
#[case::from_empty(Audio::Empty)]
#[case::from_ready(Audio::Ready)]
#[case::from_failed(Audio::Failed)]
fn generate_with_no_saved_config_does_nothing(#[case] from: Audio) {
    let mut s = composing(from);
    s.config = None;

    let outcome = s.apply(Input::GeneratePressed);

    assert_eq!(audio_of(&s), from);
    assert!(outcome.effects.is_empty());
}

#[test]
fn generate_while_generating_starts_no_second_request() {
    let mut s = composing(Audio::Generating);

    let outcome = s.apply(Input::GeneratePressed);

    assert_eq!(audio_of(&s), Audio::Generating);
    assert!(!outcome.changed);
    assert!(outcome.effects.is_empty());
}

#[test]
fn audio_that_arrives_while_generating_is_ready_with_no_play_issue() {
    let mut s = composing(Audio::Generating);

    let outcome = s.apply(Input::Generated(Ok(clip())));

    assert_eq!(ready_clip_size(&s), Some(clip().size()));
    assert_eq!(play_issue(&s), None);
    assert!(outcome.changed);
    assert!(outcome.effects.is_empty());
}

#[test]
fn a_failure_that_arrives_while_generating_is_kept_as_the_failure() {
    let mut s = composing(Audio::Generating);

    s.apply(Input::Generated(Err(WHY.to_owned())));

    assert!(matches!(s.audio(), ComposeAudio::Failed(why) if why == WHY));
}

#[rstest]
#[case::empty_ok(Audio::Empty, Ok(another_clip()))]
#[case::empty_err(Audio::Empty, Err(WHY.to_owned()))]
#[case::ready_ok(Audio::Ready, Ok(another_clip()))]
#[case::ready_err(Audio::Ready, Err(WHY.to_owned()))]
#[case::failed_ok(Audio::Failed, Ok(another_clip()))]
#[case::failed_err(Audio::Failed, Err("another failure".to_owned()))]
fn an_answer_with_no_request_on_its_way_changes_nothing(
    #[case] from: Audio,
    #[case] answer: Result<Clip, String>,
) {
    let mut s = composing(from);
    let size_before = ready_clip_size(&s);

    let outcome = s.apply(Input::Generated(answer));

    assert_eq!(audio_of(&s), from);
    assert_eq!(ready_clip_size(&s), size_before);
    assert!(!outcome.changed);
}

#[test]
fn play_when_ready_plays_the_clip_and_clears_the_play_issue() {
    let mut s = composing(Audio::Ready);
    s.apply(Input::PlayFailed(PLAY_WHY.to_owned()));

    let outcome = s.apply(Input::PlayPressed);

    assert!(
        matches!(
            outcome.effects.as_slice(),
            [Effect::Play(played)] if played.bytes() == clip().bytes()
        ),
        "{:?}",
        outcome.effects
    );
    assert_eq!(play_issue(&s), None);
    assert_eq!(audio_of(&s), Audio::Ready);
}

#[rstest]
#[case::empty(Audio::Empty)]
#[case::generating(Audio::Generating)]
#[case::failed(Audio::Failed)]
fn play_with_no_audio_plays_nothing(#[case] from: Audio) {
    let mut s = composing(from);

    let outcome = s.apply(Input::PlayPressed);

    assert_eq!(audio_of(&s), from);
    assert!(!outcome.changed);
    assert!(outcome.effects.is_empty());
}

#[test]
fn a_play_failure_when_ready_is_kept_as_the_play_issue() {
    let mut s = composing(Audio::Ready);

    let outcome = s.apply(Input::PlayFailed(PLAY_WHY.to_owned()));

    assert_eq!(play_issue(&s), Some(PLAY_WHY));
    assert!(outcome.changed);
}

#[test]
fn a_play_failure_that_arrives_after_a_newer_clip_still_shows() {
    let mut s = composing(Audio::Ready);
    s.apply(Input::PlayPressed);
    s.apply(Input::GeneratePressed);
    s.apply(Input::Generated(Ok(another_clip())));

    s.apply(Input::PlayFailed(PLAY_WHY.to_owned()));

    assert_eq!(ready_clip_size(&s), Some(another_clip().size()));
    assert_eq!(play_issue(&s), Some(PLAY_WHY));
}

#[rstest]
#[case::empty(Audio::Empty)]
#[case::generating(Audio::Generating)]
#[case::failed(Audio::Failed)]
fn a_play_failure_with_no_audio_changes_nothing(#[case] from: Audio) {
    let mut s = composing(from);

    let outcome = s.apply(Input::PlayFailed(PLAY_WHY.to_owned()));

    assert_eq!(audio_of(&s), from);
    assert!(!outcome.changed);
}

#[test]
fn editing_the_text_keeps_the_generated_audio() {
    let mut s = composing(Audio::Ready);

    s.text.insert(" and more");

    assert_eq!(ready_clip_size(&s), Some(clip().size()));
}

#[test]
fn a_config_saved_while_generating_leaves_the_request_and_its_answer_applies() {
    let mut s = configured();
    s.form.voice = TextEdit::new("another.wav");
    s.apply(Input::SavePressed);
    s.apply(Input::GeneratePressed);
    let mut saved = config();
    saved.set_voice("another.wav");

    s.apply(Input::ConfigSaved(Ok(saved.clone())));
    let while_generating = audio_of(&s);
    s.apply(Input::Generated(Ok(clip())));

    assert_eq!(while_generating, Audio::Generating);
    assert_eq!(s.config(), Some(&saved));
    assert_eq!(audio_of(&s), Audio::Ready);
}

#[test]
fn an_answer_that_arrives_while_settings_shows_still_applies() {
    let mut s = composing(Audio::Generating);
    s.apply(Input::AreaPicked(Area::Settings));

    s.apply(Input::Generated(Ok(clip())));

    assert_eq!(s.area(), Area::Settings);
    assert_eq!(audio_of(&s), Audio::Ready);
}

#[test]
fn picking_the_other_area_shows_it_and_picking_it_again_changes_nothing() {
    let mut s = configured();

    let first = s.apply(Input::AreaPicked(Area::Settings));
    let again = s.apply(Input::AreaPicked(Area::Settings));

    assert_eq!(s.area(), Area::Settings);
    assert!(first.changed);
    assert!(!again.changed);
}

#[rstest]
#[case::from_idle(SaveStatus::Idle)]
#[case::from_saved(SaveStatus::Saved)]
#[case::from_failed(SaveStatus::Failed(WHY.to_owned()))]
fn save_of_a_valid_form_starts_one_save_carrying_the_form(#[case] from: SaveStatus) {
    let mut s = saving(from);
    s.form.voice = TextEdit::new("another.wav");
    s.form.method = Method::Put;
    s.form.reactions = TextEdit::new("[laugh]\n\n  [clear throat]  \n");

    let outcome = s.apply(Input::SavePressed);

    let mut expected = config();
    expected.set_voice("another.wav");
    expected.request_method = "PUT".to_owned();
    expected.supported_vocal_reactions = vec!["[laugh]".to_owned(), "[clear throat]".to_owned()];
    assert_eq!(s.save_status(), &SaveStatus::Saving { edited: false });
    assert!(
        matches!(
            outcome.effects.as_slice(),
            [Effect::Save { config: sent, path }] if *sent == expected && *path == config_path()
        ),
        "{:?}",
        outcome.effects
    );
    assert_eq!(s.config(), Some(&config()));
}

#[rstest]
#[case::from_idle(SaveStatus::Idle)]
#[case::from_saved(SaveStatus::Saved)]
#[case::from_failed(SaveStatus::Failed(WHY.to_owned()))]
fn save_of_an_invalid_form_shows_each_fields_issue_and_saves_nothing(#[case] from: SaveStatus) {
    let mut s = saving(from);
    s.form.server = TextEdit::new("ftp://my-host");
    s.form.model = TextEdit::new("  ");
    s.form.voice = TextEdit::new("");
    s.form.reactions = TextEdit::new("[laugh]\nchuckle");

    let outcome = s.apply(Input::SavePressed);

    assert_eq!(s.save_status(), &SaveStatus::Idle);
    assert!(outcome.effects.is_empty());
    assert_eq!(
        s.issues(),
        &FieldIssues {
            server: Some("The speech server URL must start with http:// or https://".to_owned()),
            model: Some("The payload's model must be a text that is not blank".to_owned()),
            voice: Some("The payload's voice must be a text that is not blank".to_owned()),
            reactions: Some("Line 2 is not a tag in square brackets, like [laugh]".to_owned()),
        }
    );
    assert_eq!(s.issues().first(), Some(Field::Server));
}

#[rstest]
#[case::as_the_file_has_it(SERVER, SPEECH_PATH, "http://my-host:10157/v1/audio/speech")]
#[case::no_scheme_no_leading_slash("my-host:10157", "speak", "http://my-host:10157/speak")]
#[case::https_with_a_query("https://my-host", "/v1/tts?lang=en", "https://my-host/v1/tts?lang=en")]
fn save_joins_the_server_and_the_endpoint_into_the_url(
    #[case] server: &str,
    #[case] endpoint: &str,
    #[case] url: &str,
) {
    let mut s = saving(SaveStatus::Idle);
    s.form.server = TextEdit::new(server);
    s.form.endpoint = TextEdit::new(endpoint);

    let outcome = s.apply(Input::SavePressed);

    let expected = SpeakConfig {
        url: url.to_owned(),
        ..config()
    };
    assert!(
        matches!(
            outcome.effects.as_slice(),
            [Effect::Save { config: sent, .. }] if *sent == expected
        ),
        "{:?}",
        outcome.effects
    );
}

#[test]
fn save_with_a_bad_server_shows_its_issue_alone_and_saves_nothing() {
    let mut s = saving(SaveStatus::Saved);
    s.form.server = TextEdit::new("http://");

    let outcome = s.apply(Input::SavePressed);

    assert_eq!(s.save_status(), &SaveStatus::Idle);
    assert!(outcome.effects.is_empty());
    assert_eq!(
        s.issues(),
        &FieldIssues {
            server: Some("The speech server URL is not a valid URL".to_owned()),
            ..FieldIssues::default()
        }
    );
}

#[test]
fn a_loaded_url_that_is_not_http_shows_cut_and_save_reports_it_on_the_server() {
    let loaded = SpeakConfig {
        url: "ftp://my-host/speech".to_owned(),
        ..config()
    };
    let mut s = SpeakState::new(Some(loaded), config_path(), None);
    let shown = (s.form.server.text.clone(), s.form.endpoint.text.clone());

    let outcome = s.apply(Input::SavePressed);

    assert_eq!(shown, ("ftp://my-host".to_owned(), "/speech".to_owned()));
    assert!(outcome.effects.is_empty());
    assert_eq!(s.issues().first(), Some(Field::Server));
    assert!(s.issues().server.is_some());
}

#[test]
fn a_save_keeps_payload_fields_the_form_does_not_show() {
    let mut loaded = config();
    loaded.payload.insert("speed".to_owned(), json!(1.5));
    let mut s = SpeakState::new(Some(loaded.clone()), config_path(), None);

    let outcome = s.apply(Input::SavePressed);

    assert!(
        matches!(
            outcome.effects.as_slice(),
            [Effect::Save { config: sent, .. }] if *sent == loaded
        ),
        "{:?}",
        outcome.effects
    );
}

#[test]
fn save_while_saving_starts_no_second_save() {
    let mut s = saving(SaveStatus::Saving { edited: false });

    let outcome = s.apply(Input::SavePressed);

    assert_eq!(s.save_status(), &SaveStatus::Saving { edited: false });
    assert!(outcome.effects.is_empty());
}

#[test]
fn a_save_that_lands_with_no_edit_since_is_saved_and_replaces_the_config() {
    let mut s = saving(SaveStatus::Saving { edited: false });
    let mut written = config();
    written.set_voice("another.wav");

    s.apply(Input::ConfigSaved(Ok(written.clone())));

    assert_eq!(s.save_status(), &SaveStatus::Saved);
    assert_eq!(s.config(), Some(&written));
}

#[test]
fn a_save_that_lands_after_an_edit_is_idle_and_replaces_the_config() {
    let mut s = saving(SaveStatus::Saving { edited: false });
    let mut written = config();
    written.set_voice("another.wav");
    s.apply(Input::FieldEdited(Field::Voice));
    let while_saving = s.save_status().clone();

    s.apply(Input::ConfigSaved(Ok(written.clone())));

    assert_eq!(while_saving, SaveStatus::Saving { edited: true });
    assert_eq!(s.save_status(), &SaveStatus::Idle);
    assert_eq!(s.config(), Some(&written));
}

#[rstest]
#[case::no_edit_since(false)]
#[case::edited_since(true)]
fn a_save_that_fails_is_failed_and_keeps_the_config(#[case] edited: bool) {
    let mut s = saving(SaveStatus::Saving { edited });

    s.apply(Input::ConfigSaved(Err(WHY.to_owned())));

    assert_eq!(s.save_status(), &SaveStatus::Failed(WHY.to_owned()));
    assert_eq!(s.config(), Some(&config()));
}

#[rstest]
#[case::after_saved(SaveStatus::Saved)]
#[case::after_failed(SaveStatus::Failed(WHY.to_owned()))]
fn an_edit_after_a_finished_save_returns_to_idle(#[case] from: SaveStatus) {
    let mut s = saving(from);

    let outcome = s.apply(Input::FieldEdited(Field::Model));

    assert_eq!(s.save_status(), &SaveStatus::Idle);
    assert!(outcome.changed);
}

#[test]
fn an_edit_while_idle_with_no_issue_changes_nothing() {
    let mut s = saving(SaveStatus::Idle);

    let outcome = s.apply(Input::FieldEdited(Field::Method));

    assert_eq!(s.save_status(), &SaveStatus::Idle);
    assert!(!outcome.changed);
}

#[rstest]
#[case::idle_ok(SaveStatus::Idle, Ok(SpeakConfig::default()))]
#[case::idle_err(SaveStatus::Idle, Err(WHY.to_owned()))]
#[case::saved_ok(SaveStatus::Saved, Ok(SpeakConfig::default()))]
#[case::saved_err(SaveStatus::Saved, Err(WHY.to_owned()))]
#[case::failed_ok(SaveStatus::Failed(WHY.to_owned()), Ok(SpeakConfig::default()))]
#[case::failed_err(SaveStatus::Failed(WHY.to_owned()), Err("another failure".to_owned()))]
fn a_save_answer_with_no_save_on_its_way_changes_nothing(
    #[case] from: SaveStatus,
    #[case] answer: Result<SpeakConfig, String>,
) {
    let mut s = saving(from.clone());

    let outcome = s.apply(Input::ConfigSaved(answer));

    assert_eq!(s.save_status(), &from);
    assert_eq!(s.config(), Some(&config()));
    assert!(!outcome.changed);
}

#[rstest]
#[case::server(Field::Server, FieldIssues { server: None, ..all_issues() })]
#[case::endpoint(Field::Endpoint, FieldIssues { server: None, ..all_issues() })]
#[case::model(Field::Model, FieldIssues { model: None, ..all_issues() })]
#[case::voice(Field::Voice, FieldIssues { voice: None, ..all_issues() })]
#[case::reactions(Field::Reactions, FieldIssues { reactions: None, ..all_issues() })]
#[case::method(Field::Method, all_issues())]
fn editing_a_field_clears_only_the_issue_it_answers(
    #[case] field: Field,
    #[case] left: FieldIssues,
) {
    let mut s = configured();
    s.issues = all_issues();

    s.apply(Input::FieldEdited(field));

    assert_eq!(s.issues(), &left);
}

fn all_issues() -> FieldIssues {
    FieldIssues {
        server: Some("server issue".to_owned()),
        model: Some("model issue".to_owned()),
        voice: Some("voice issue".to_owned()),
        reactions: Some("reactions issue".to_owned()),
    }
}

#[rstest]
#[case::at_the_start_of_the_text("", "[laugh]")]
#[case::after_a_space("Hello ", "[laugh]")]
#[case::after_a_letter("Hello", " [laugh]")]
#[case::after_punctuation("Hey, please pay attention!", " [laugh]")]
#[case::after_a_line_break("Hello\n", "[laugh]")]
#[case::after_another_tag("Hello [sigh]", " [laugh]")]
fn reaction_insert_adds_one_space_only_after_a_word(#[case] before: &str, #[case] typed: &str) {
    assert_eq!(reaction_insert(before, "[laugh]"), typed);
}

#[test]
fn inserting_a_reaction_at_the_end_gives_the_text_then_a_space_then_the_tag() {
    let mut s = configured();
    s.text = TextEdit::new("Hey, please pay attention!");

    s.insert_reaction("[clear throat]");

    assert_eq!(s.text.text, "Hey, please pay attention! [clear throat]");
    assert_eq!(s.text.range(), (s.text.text.len(), s.text.text.len()));
}

#[test]
fn inserting_a_reaction_replaces_the_selection_and_leaves_the_caret_after_the_tag() {
    let mut s = configured();
    s.text = TextEdit::new("Hello there world");
    s.text.set_selection(6, 11);

    s.insert_reaction("[sigh]");

    assert_eq!(s.text.text, "Hello [sigh] world");
    assert_eq!(s.text.range(), (12, 12));
}

#[test]
fn parse_reactions_trims_lines_and_drops_blank_ones() {
    let reactions = parse_reactions("[laugh]\n\n   \n  [clear throat]  \r\n[sigh]");

    assert_eq!(
        reactions,
        Ok(vec![
            "[laugh]".to_owned(),
            "[clear throat]".to_owned(),
            "[sigh]".to_owned()
        ])
    );
}

#[test]
fn parse_reactions_of_an_empty_box_is_an_empty_list() {
    assert_eq!(parse_reactions(" \n"), Ok(Vec::new()));
}

#[rstest]
#[case::no_brackets("[laugh]\nchuckle", 2)]
#[case::only_an_opening_bracket("[laugh", 1)]
#[case::only_a_closing_bracket("\nlaugh]", 2)]
#[case::empty_brackets("[laugh]\n[sigh]\n[ ]", 3)]
#[case::two_tags_on_a_line("[laugh] [sigh]", 1)]
#[case::text_after_the_tag("[laugh] loudly", 1)]
fn parse_reactions_names_the_first_line_that_is_not_a_tag(
    #[case] lines: &str,
    #[case] line: usize,
) {
    let issue = parse_reactions(lines).unwrap_err();

    assert_eq!(
        issue,
        format!("Line {line} is not a tag in square brackets, like [laugh]")
    );
}

#[rstest]
#[case::one_word("[laugh]", "laugh")]
#[case::two_words("[clear throat]", "clear throat")]
#[case::spaces_inside(" [ sigh ] ", "sigh")]
fn reaction_word_is_the_tag_without_its_brackets(#[case] tag: &str, #[case] word: &str) {
    assert_eq!(reaction_word(tag), word);
}

#[test]
fn a_first_run_opens_on_settings_with_the_defaults_and_an_empty_server() {
    let s = first_run();

    assert_eq!(s.area(), Area::Settings);
    assert_eq!(s.form.server.text, "");
    assert_eq!(s.form.endpoint.text, SPEECH_PATH);
    assert_eq!(s.form.model.text, "chatterbox-turbo");
    assert_eq!(s.form.voice.text, "sj-clip-001.wav");
    assert_eq!(s.form.method, Method::Post);
    assert_eq!(s.form.reactions.text.lines().count(), 9);
    assert_eq!(s.reactions().len(), 9);
    assert!(!s.can_generate());
    assert_eq!(
        s.compose_status(),
        Some((Tone::Muted, FIRST_RUN_STATUS.to_owned()))
    );
}

#[test]
fn the_first_save_of_a_first_run_ends_the_first_run_status_and_enables_generate() {
    let mut s = first_run();
    s.form.server = TextEdit::new(SERVER);
    let outcome = s.apply(Input::SavePressed);
    let Some(Effect::Save { config: sent, .. }) = outcome.effects.into_iter().next() else {
        panic!("the first save started no save");
    };

    s.apply(Input::ConfigSaved(Ok(sent)));

    assert_eq!(s.config(), Some(&config()));
    assert_eq!(s.compose_status(), None);
    assert!(s.can_generate());
}

#[test]
fn a_loaded_config_opens_on_compose_with_settings_filled_from_it() {
    let mut loaded = config();
    loaded.request_method = "PUT".to_owned();
    loaded.set_model("another-model");
    loaded.supported_vocal_reactions = vec!["[hum]".to_owned()];

    let s = SpeakState::new(Some(loaded), config_path(), None);

    assert_eq!(s.area(), Area::Compose);
    assert_eq!(s.form.server.text, SERVER);
    assert_eq!(s.form.endpoint.text, SPEECH_PATH);
    assert_eq!(s.form.method, Method::Put);
    assert_eq!(s.form.model.text, "another-model");
    assert_eq!(s.form.reactions.text, "[hum]");
    assert_eq!(s.reactions(), vec!["[hum]".to_owned()]);
    assert_eq!(s.compose_status(), None);
}

#[test]
fn a_method_the_file_spells_another_way_shows_as_post() {
    let mut loaded = config();
    loaded.request_method = "DELETE".to_owned();

    let s = SpeakState::new(Some(loaded), config_path(), None);

    assert_eq!(s.form.method, Method::Post);
}

#[rstest]
#[case::generating(Audio::Generating, Some((Tone::Muted, "Generating…".to_owned())))]
#[case::ready(Audio::Ready, Some((Tone::Muted, "Ready to play".to_owned())))]
#[case::failed(Audio::Failed, Some((Tone::Danger, format!("Couldn't generate: {WHY}"))))]
#[case::empty(Audio::Empty, None)]
fn compose_status_says_where_the_audio_is(
    #[case] audio: Audio,
    #[case] words: Option<(Tone, String)>,
) {
    assert_eq!(composing(audio).compose_status(), words);
}

#[test]
fn compose_status_says_why_the_last_play_failed() {
    let mut s = composing(Audio::Ready);
    s.apply(Input::PlayFailed(PLAY_WHY.to_owned()));

    assert_eq!(
        s.compose_status(),
        Some((Tone::Danger, format!("Couldn't play: {PLAY_WHY}")))
    );
}

#[rstest]
#[case::idle(SaveStatus::Idle, None)]
#[case::saving(SaveStatus::Saving { edited: false }, None)]
#[case::saved(SaveStatus::Saved, Some((Tone::Muted, "Saved".to_owned())))]
#[case::failed(
    SaveStatus::Failed(WHY.to_owned()),
    Some((Tone::Danger, format!("Couldn't save: {WHY}")))
)]
fn save_words_say_how_the_last_save_went(
    #[case] save: SaveStatus,
    #[case] words: Option<(Tone, String)>,
) {
    assert_eq!(saving(save).save_words(), words);
}

#[rstest]
#[case::post(Method::Post, "POST")]
#[case::put(Method::Put, "PUT")]
fn a_method_round_trips_through_its_label(#[case] method: Method, #[case] label: &str) {
    assert_eq!(method.label(), label);
    assert_eq!(Method::from_label(label), Some(method));
}

#[rstest]
#[case::compose(Area::Compose, 0, "compose")]
#[case::settings(Area::Settings, 1, "settings")]
fn an_area_has_its_place_in_the_switch_and_its_log_word(
    #[case] area: Area,
    #[case] index: usize,
    #[case] word: &str,
) {
    assert_eq!(Area::ALL[area.index()], area);
    assert_eq!(area.index(), index);
    assert_eq!(area.as_str(), word);
}
