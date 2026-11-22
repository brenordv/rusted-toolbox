use super::*;
use crate::config;
use crate::gui::state::{Area, ComposeAudio, SaveStatus};
use crate::gui::test_support::{Call, clip, config as server_config, config_path, recording};
use crate::test_support::{Listener, Reply};
use anyhow::bail;
use common_audio::test_support::tone_wav;
use rune::TextEdit;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SPOKEN: &str = "Hello from the tests";
const NO_DEVICE: &str = "No sound output device could be opened";
const ANSWER_WAIT: Duration = Duration::from_secs(10);

type Played = Arc<Mutex<Vec<usize>>>;

struct RecordingPlayer {
    played: Played,
    fails: bool,
}

impl Player for RecordingPlayer {
    fn play(&mut self, clip: &Clip) -> Result<()> {
        if self.fails {
            bail!("The sound could not be decoded");
        }
        self.played.lock().unwrap().push(clip.size());
        Ok(())
    }
}

fn failure_of(input: Input) -> Option<String> {
    match input {
        Input::PlayFailed(why) => Some(why),
        Input::Generated(Err(why)) | Input::ConfigSaved(Err(why)) => Some(why),
        Input::Generated(Ok(_))
        | Input::ConfigSaved(Ok(_))
        | Input::AreaPicked(_)
        | Input::GeneratePressed
        | Input::PlayPressed
        | Input::SavePressed
        | Input::FieldEdited(_) => None,
    }
}

async fn dead_task() -> JoinError {
    let task = tokio::spawn(std::future::pending::<Input>());
    task.abort();
    task.await.unwrap_err()
}

#[tokio::test]
async fn generated_against_a_server_that_answers_audio_is_the_clip() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let client = SpeechClient::new().unwrap();

    let answer = generated(&client, &listener.config(), SPOKEN).await;

    assert!(
        matches!(&answer, Input::Generated(Ok(clip)) if clip.size() == tone_wav().len()),
        "{answer:?}"
    );
    assert_eq!(listener.received()[0].json()["input"], SPOKEN);
}

#[tokio::test]
async fn generated_against_a_server_error_is_the_failure_with_its_detail_cut() {
    let listener = Listener::start(Reply::json(404, &json!({"detail": "d".repeat(500)})));
    let client = SpeechClient::new().unwrap();

    let answer = generated(&client, &listener.config(), SPOKEN).await;

    let why = failure_of(answer).unwrap();
    assert!(why.contains("answered 404 Not Found: ddd"), "{why}");
    assert!(why.ends_with('…'), "{why}");
    assert!(!why.contains(SPOKEN), "{why}");
}

#[tokio::test]
async fn generated_from_an_answer_that_is_not_audio_is_a_failure_naming_its_content_type() {
    let listener = Listener::start(Reply::new(200, "text/plain", "not audio"));
    let client = SpeechClient::new().unwrap();

    let answer = generated(&client, &listener.config(), SPOKEN).await;

    let why = failure_of(answer).unwrap();
    assert!(why.contains("content type text/plain"), "{why}");
}

#[tokio::test]
async fn saved_writes_the_file_load_reads_back_and_answers_with_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool-speak").join("config.json");

    let answer = saved(server_config(), path.clone()).await;

    assert!(
        matches!(&answer, Input::ConfigSaved(Ok(written)) if *written == server_config()),
        "{answer:?}"
    );
    assert_eq!(config::load(&path).await.unwrap(), Some(server_config()));
}

#[tokio::test]
async fn saved_into_a_path_that_cannot_be_written_is_the_failure_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("a-file");
    std::fs::write(&blocker, "in the way").unwrap();
    let path = blocker.join("config.json");

    let answer = saved(server_config(), path.clone()).await;

    let why = failure_of(answer).unwrap();
    assert!(why.contains(&path.display().to_string()), "{why}");
    assert_eq!(std::fs::read_to_string(&blocker).unwrap(), "in the way");
}

#[tokio::test]
async fn a_generate_task_that_died_is_answered_with_one_generate_failure() {
    let answer = answer_of(Err(dead_task().await), STEP_GENERATE, generate_failed);

    assert!(
        matches!(
            &answer,
            Input::Generated(Err(why)) if why.starts_with("The generate task stopped unexpectedly")
        ),
        "{answer:?}"
    );
}

#[tokio::test]
async fn a_save_task_that_died_is_answered_with_one_save_failure() {
    let answer = answer_of(Err(dead_task().await), STEP_SAVE, save_failed);

    assert!(
        matches!(
            &answer,
            Input::ConfigSaved(Err(why)) if why.starts_with("The save task stopped unexpectedly")
        ),
        "{answer:?}"
    );
}

#[test]
fn a_task_that_answered_keeps_its_answer() {
    let answer = answer_of(Ok(Input::PlayPressed), STEP_SAVE, save_failed);

    assert!(matches!(answer, Input::PlayPressed), "{answer:?}");
}

#[test]
fn an_opener_that_fails_posts_one_play_failure_with_its_text() {
    let (post, posted) = mpsc::channel();
    let (clips, thread) = spawn_audio(
        || -> Result<RecordingPlayer> { bail!(NO_DEVICE) },
        move |input| post.send(input).unwrap(),
    );

    clips.send(clip()).unwrap();
    let first = posted.recv_timeout(ANSWER_WAIT).unwrap();
    drop(clips);
    thread.join().unwrap();

    assert_eq!(failure_of(first).as_deref(), Some(NO_DEVICE));
    assert!(posted.try_recv().is_err());
}

#[test]
fn two_clips_in_a_row_open_the_device_once_and_post_nothing() {
    let played = Played::default();
    let opened = Arc::new(AtomicUsize::new(0));
    let (record, count) = (Arc::clone(&played), Arc::clone(&opened));
    let (post, posted) = mpsc::channel();
    let (clips, thread) = spawn_audio(
        move || {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(RecordingPlayer {
                played: Arc::clone(&record),
                fails: false,
            })
        },
        move |input| post.send(input).unwrap(),
    );

    clips.send(clip()).unwrap();
    clips.send(clip()).unwrap();
    drop(clips);
    thread.join().unwrap();

    assert_eq!(opened.load(Ordering::SeqCst), 1);
    assert_eq!(*played.lock().unwrap(), vec![clip().size(), clip().size()]);
    assert!(posted.try_recv().is_err());
}

#[test]
fn a_device_that_opens_on_the_second_try_plays_the_second_clip() {
    let played = Played::default();
    let opened = Arc::new(AtomicUsize::new(0));
    let (record, count) = (Arc::clone(&played), Arc::clone(&opened));
    let (post, posted) = mpsc::channel();
    let (clips, thread) = spawn_audio(
        move || {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                bail!(NO_DEVICE);
            }
            Ok(RecordingPlayer {
                played: Arc::clone(&record),
                fails: false,
            })
        },
        move |input| post.send(input).unwrap(),
    );

    clips.send(clip()).unwrap();
    clips.send(clip()).unwrap();
    drop(clips);
    thread.join().unwrap();

    let failures: Vec<_> = posted.try_iter().filter_map(failure_of).collect();
    assert_eq!(failures, vec![NO_DEVICE.to_owned()]);
    assert_eq!(*played.lock().unwrap(), vec![clip().size()]);
}

#[test]
fn a_clip_the_player_cannot_play_posts_a_play_failure() {
    let (post, posted) = mpsc::channel();
    let (clips, thread) = spawn_audio(
        || {
            Ok(RecordingPlayer {
                played: Played::default(),
                fails: true,
            })
        },
        move |input| post.send(input).unwrap(),
    );

    clips.send(clip()).unwrap();
    drop(clips);
    thread.join().unwrap();

    let failures: Vec<_> = posted.try_iter().filter_map(failure_of).collect();
    assert_eq!(failures, vec!["The sound could not be decoded".to_owned()]);
}

#[test]
fn a_clip_sent_to_a_running_audio_thread_needs_no_answer() {
    let (audio, clips) = mpsc::channel();

    let failure = send_clip(&audio, clip());

    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(clips.try_recv().unwrap().size(), clip().size());
}

#[test]
fn a_clip_sent_after_the_audio_thread_stopped_is_answered_with_a_play_failure() {
    let (audio, clips) = mpsc::channel();
    drop(clips);

    let failure = send_clip(&audio, clip());

    assert_eq!(failure.and_then(failure_of).as_deref(), Some(AUDIO_STOPPED));
}

#[test]
fn dispatch_hands_each_effect_to_the_services() {
    let (mut s, services) = recording(Some(server_config()));
    s.text = TextEdit::new(SPOKEN);

    dispatch(&mut s, Input::GeneratePressed);
    dispatch(&mut s, Input::Generated(Ok(clip())));
    dispatch(&mut s, Input::PlayPressed);
    dispatch(&mut s, Input::SavePressed);

    assert_eq!(
        services.calls(),
        vec![
            Call::Generate {
                config: server_config(),
                text: SPOKEN.to_owned(),
            },
            Call::Play(clip().size()),
            Call::Save {
                config: server_config(),
                path: config_path(),
            },
        ]
    );
}

#[test]
fn dispatch_reports_whether_the_view_changed() {
    let (mut s, _) = recording(Some(server_config()));

    let changed = dispatch(&mut s, Input::AreaPicked(Area::Settings));
    let again = dispatch(&mut s, Input::AreaPicked(Area::Settings));

    assert!(changed);
    assert!(!again);
}

#[test]
fn a_window_with_no_services_answers_each_request_with_a_failure() {
    let mut s = SpeakState::new(Some(server_config()), config_path(), None);
    s.text = TextEdit::new(SPOKEN);

    dispatch(&mut s, Input::GeneratePressed);
    dispatch(&mut s, Input::SavePressed);

    assert!(matches!(s.audio(), ComposeAudio::Failed(why) if why == NOT_SET_UP));
    assert_eq!(s.save_status(), &SaveStatus::Failed(NOT_SET_UP.to_owned()));
}

#[test]
fn a_window_with_no_services_reports_a_play_as_failed() {
    let (mut ready, _) = recording(Some(server_config()));
    ready.text = TextEdit::new(SPOKEN);
    ready.apply(Input::GeneratePressed);
    ready.apply(Input::Generated(Ok(clip())));
    ready.services = None;

    dispatch(&mut ready, Input::PlayPressed);

    assert!(matches!(
        ready.audio(),
        ComposeAudio::Ready { play_issue: Some(why), .. } if why == NOT_SET_UP
    ));
}
