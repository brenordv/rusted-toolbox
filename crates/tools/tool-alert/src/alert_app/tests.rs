use super::*;
use common_audio::test_support::tone_wav;
use rstest::rstest;
use std::cell::Cell;
use std::sync::mpsc;
use std::time::Duration;

const PLAY_FAILURE: &str = "No sound output device could be opened";
const ANSWER_WAIT: Duration = Duration::from_secs(10);

fn args(title: Option<&str>, body: Option<&str>, sound: Option<&str>, gui: bool) -> AlertArgs {
    AlertArgs {
        title: title.map(str::to_owned),
        body: body.map(str::to_owned),
        sound: sound.map(str::to_owned),
        gui,
    }
}

#[test]
fn a_body_with_a_title_and_a_sound_shows_an_alert() {
    let mode = mode(args(
        Some("Done"),
        Some("Build finished"),
        Some("ping.wav"),
        false,
    ))
    .unwrap();

    assert_eq!(
        mode,
        Mode::Show(AlertSpec {
            title: Some("Done".to_owned()),
            body: "Build finished".to_owned(),
            sound: Some(PathBuf::from("ping.wav")),
        })
    );
}

#[test]
fn a_blank_title_and_a_blank_sound_are_left_out() {
    let mode = mode(args(Some("  "), Some("Build finished"), Some(""), false)).unwrap();

    assert_eq!(
        mode,
        Mode::Show(AlertSpec {
            title: None,
            body: "Build finished".to_owned(),
            sound: None,
        })
    );
}

#[rstest]
#[case::blank(Some("  \n"))]
#[case::empty(Some(""))]
#[case::missing(None)]
fn a_blank_body_is_refused(#[case] body: Option<&str>) {
    let result = mode(args(Some("Done"), body, None, false));

    assert_eq!(result.unwrap_err().to_string(), BLANK_BODY);
}

#[test]
fn gui_opens_the_tester_filled_with_what_was_given() {
    let mode = mode(args(Some("Done"), None, Some("ping.wav"), true)).unwrap();

    assert_eq!(
        mode,
        Mode::Tester(TesterSeed {
            title: "Done".to_owned(),
            body: String::new(),
            sound: "ping.wav".to_owned(),
        })
    );
}

#[test]
fn a_sound_that_plays_has_no_issue() {
    let issue = sound_issue(Path::new("ping.wav"), |_| Ok(()));

    assert_eq!(issue, None);
}

#[test]
fn a_sound_that_fails_to_play_gives_the_failure_text() {
    let issue = sound_issue(Path::new("ping.wav"), |_| bail!(PLAY_FAILURE));

    assert_eq!(issue.as_deref(), Some(PLAY_FAILURE));
}

#[test]
fn a_missing_sound_file_gives_an_error_naming_the_path() {
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("missing.wav");

    let issue = sound_issue(&missing, |path| Clip::from_file(path).map(|_| ())).unwrap();

    assert!(issue.contains("missing.wav"), "{issue}");
}

#[test]
fn a_sound_file_that_exists_loads_as_a_clip() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("tone.wav");
    std::fs::write(&path, tone_wav()).unwrap();

    let issue = sound_issue(&path, |path| Clip::from_file(path).map(|_| ()));

    assert_eq!(issue, None);
}

#[cfg(windows)]
#[rstest]
#[case::backslashes(r"\\host\share\x.wav")]
#[case::slashes("//host/share/x.wav")]
#[case::mixed(r"\/host\share\x.wav")]
#[case::mixed_the_other_way(r"/\host\share\x.wav")]
fn a_network_share_sound_is_refused_without_playing_it(#[case] path: &str) {
    let played = Cell::new(false);

    let issue = sound_issue(Path::new(path), |_| {
        played.set(true);
        Ok(())
    });

    assert_eq!(issue.as_deref(), Some(NETWORK_SHARE_REFUSED));
    assert!(!played.get());
}

#[cfg(not(windows))]
#[test]
fn two_leading_slashes_are_an_ordinary_path_off_windows() {
    let played = Cell::new(false);

    let issue = sound_issue(Path::new("//host/share/x.wav"), |_| {
        played.set(true);
        Ok(())
    });

    assert_eq!(issue, None);
    assert!(played.get());
}

#[rstest]
#[case::two_backslashes(r"\\host\share", true)]
#[case::two_slashes("//host/share", true)]
#[case::mixed(r"\/host", true)]
#[case::a_drive_path(r"C:\sounds\ping.wav", false)]
#[case::one_slash("/sounds/ping.wav", false)]
#[case::a_relative_path("sounds/ping.wav", false)]
#[case::one_character("/", false)]
#[case::empty("", false)]
fn only_two_leading_separators_start_like_a_network_share(
    #[case] path: &str,
    #[case] expected: bool,
) {
    assert_eq!(starts_like_a_network_share(path), expected);
}

#[test]
fn a_started_sound_that_fails_posts_its_issue_once() {
    let (post, posted) = mpsc::channel();

    start_sound(
        PathBuf::from("ping.wav"),
        |_| bail!(PLAY_FAILURE),
        move |issue| post.send(issue).unwrap(),
    );

    let first = posted.recv_timeout(ANSWER_WAIT).unwrap();
    assert_eq!(first.as_deref(), Some(PLAY_FAILURE));
    assert!(posted.recv_timeout(ANSWER_WAIT).is_err());
}

#[test]
fn a_started_sound_that_plays_posts_no_issue() {
    let (post, posted) = mpsc::channel();

    start_sound(
        PathBuf::from("ping.wav"),
        |_| Ok(()),
        move |issue| post.send(issue).unwrap(),
    );

    assert_eq!(posted.recv_timeout(ANSWER_WAIT).unwrap(), None);
}

#[test]
fn the_exit_code_is_zero_unless_the_window_could_not_be_created() {
    assert_eq!(exit_code(None), 0);
    assert_eq!(exit_code(Some("no adapter")), 1);
}

#[test]
fn the_gpu_hint_names_the_backends() {
    assert!(gpu_hint().contains(GPU_BACKENDS));
}
