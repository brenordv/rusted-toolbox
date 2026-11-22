use super::*;
use crate::test_support::{Listener, Reply};
use anyhow::anyhow;
use common_audio::test_support::tone_wav;
use rstest::rstest;
use serde_json::json;
use std::cell::RefCell;
use std::io::{self, Cursor};
use std::rc::Rc;
use tempfile::TempDir;

const SPOKEN: &str = "Hello from the tests";

type Played = Rc<RefCell<Vec<Clip>>>;

struct UnreadableStdin;

impl Read for UnreadableStdin {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("stdin must not be read"))
    }
}

impl BufRead for UnreadableStdin {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(io::Error::other("stdin must not be read"))
    }

    fn consume(&mut self, _: usize) {}
}

struct Run {
    dir: TempDir,
    played: Played,
    prompt: Vec<u8>,
    stdin_terminal: bool,
    stderr_terminal: bool,
    stdin: String,
    play_fails: bool,
}

impl Run {
    /// A run from an agent: nothing is a terminal and stdin is empty.
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            played: Played::default(),
            prompt: Vec::new(),
            stdin_terminal: false,
            stderr_terminal: false,
            stdin: String::new(),
            play_fails: false,
        }
    }

    fn in_a_terminal(mut self, typed: &str) -> Self {
        self.stdin_terminal = true;
        self.stderr_terminal = true;
        self.stdin = typed.to_owned();
        self
    }

    fn piping(mut self, text: &str) -> Self {
        self.stdin = text.to_owned();
        self
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("tool-speak").join("config.json")
    }

    fn with_config(self, config: &SpeakConfig) -> Self {
        config.save(&self.config_path()).unwrap();
        self
    }

    fn config_text(&self) -> String {
        std::fs::read_to_string(self.config_path()).unwrap()
    }

    async fn speak(&mut self, args: SpeakArgs) -> Result<()> {
        let played = Rc::clone(&self.played);
        let play_fails = self.play_fails;
        let deps = Deps {
            config_path: self.config_path(),
            stdin_terminal: self.stdin_terminal,
            stderr_terminal: self.stderr_terminal,
            stdin: Cursor::new(self.stdin.clone().into_bytes()),
            prompt: &mut self.prompt,
            play: move |clip: Clip| {
                played.borrow_mut().push(clip);
                if play_fails {
                    return Err(anyhow!("No sound output device could be opened"));
                }
                Ok(())
            },
        };
        run(args, deps).await
    }
}

fn text(text: &str) -> SpeakArgs {
    SpeakArgs {
        text: Some(text.to_owned()),
        ..SpeakArgs::default()
    }
}

#[test]
fn resolve_text_returns_the_argument_without_reading_stdin() {
    let resolved = resolve_text(
        Some("  Hello  ".to_owned()),
        false,
        &mut UnreadableStdin,
        64,
    )
    .unwrap();

    assert_eq!(resolved, "Hello");
}

#[test]
fn resolve_text_reads_a_piped_stdin_when_there_is_no_argument() {
    let mut stdin = Cursor::new(b"Hello from a pipe\n".to_vec());

    let resolved = resolve_text(None, false, &mut stdin, 64).unwrap();

    assert_eq!(resolved, "Hello from a pipe");
}

#[test]
fn resolve_text_with_no_argument_at_a_terminal_says_what_to_pass_and_reads_nothing() {
    let error = resolve_text(None, true, &mut UnreadableStdin, 64).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("pass it as an argument or pipe it on stdin"),
        "{error:#}"
    );
}

#[rstest]
#[case::bytes(8, "8 byte limit")]
#[case::megabytes(2 * 1024 * 1024, "2 MB limit")]
fn resolve_text_refuses_piped_text_over_the_cap(#[case] cap: usize, #[case] limit: &str) {
    let mut stdin = Cursor::new(vec![b'a'; cap + 1]);

    let error = resolve_text(None, false, &mut stdin, cap).unwrap_err();

    assert!(error.to_string().contains(limit), "{error:#}");
}

#[test]
fn resolve_text_accepts_piped_text_exactly_at_the_cap() {
    let mut stdin = Cursor::new(b"12345678".to_vec());

    let resolved = resolve_text(None, false, &mut stdin, 8).unwrap();

    assert_eq!(resolved, "12345678");
}

#[test]
fn resolve_text_refuses_piped_bytes_that_are_not_utf8() {
    let mut stdin = Cursor::new(vec![0xff, 0xfe, 0x41]);

    let error = resolve_text(None, false, &mut stdin, 64).unwrap_err();

    assert!(error.to_string().contains("not valid UTF-8"), "{error:#}");
}

#[test]
fn resolve_text_reports_a_stdin_that_cannot_be_read() {
    let error = resolve_text(None, false, &mut UnreadableStdin, 64).unwrap_err();

    assert!(format!("{error:#}").contains("Failed to read the text from stdin"));
}

#[rstest]
#[case::from_the_argument(Some("   "), "")]
#[case::from_stdin(None, " \n\t")]
#[tokio::test]
async fn blank_text_fails_before_the_config_is_read(
    #[case] argument: Option<&str>,
    #[case] piped: &str,
) {
    let mut run = Run::new().piping(piped);
    std::fs::create_dir_all(run.config_path().parent().unwrap()).unwrap();
    std::fs::write(run.config_path(), "{ not json").unwrap();

    let error = run
        .speak(SpeakArgs {
            text: argument.map(str::to_owned),
            ..SpeakArgs::default()
        })
        .await
        .unwrap_err();

    assert_eq!(error.to_string(), "The text to speak is blank");
    assert!(run.played.borrow().is_empty());
}

#[tokio::test]
async fn no_config_and_nobody_to_ask_gives_the_two_ways_out_and_creates_nothing() {
    let mut run = Run::new();

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("Run `speak` once in a terminal"),
        "{message}"
    );
    assert!(message.contains("`speak --gui`"), "{message}");
    assert!(!run.config_path().exists());
    assert!(run.played.borrow().is_empty());
}

#[tokio::test]
async fn piped_text_with_no_config_refuses_with_the_two_ways_out() {
    let mut run = Run::new().piping(SPOKEN);

    let error = run.speak(SpeakArgs::default()).await.unwrap_err();

    assert_eq!(error.to_string(), FIRST_RUN_REFUSAL);
    assert!(!run.config_path().exists());
}

#[tokio::test]
async fn a_first_run_in_a_terminal_asks_saves_says_where_and_speaks() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut run = Run::new().in_a_terminal(&format!("127.0.0.1\n{}\n", listener.addr.port()));

    run.speak(text(SPOKEN)).await.unwrap();

    let saved = config::load(&run.config_path()).await.unwrap().unwrap();
    assert_eq!(saved.url, listener.url(""));
    let prompt = String::from_utf8(run.prompt.clone()).unwrap();
    assert!(
        prompt.contains(&run.config_path().display().to_string()),
        "{prompt}"
    );
    assert_eq!(listener.received()[0].json()["input"], SPOKEN);
    let played = run.played.borrow();
    assert_eq!(played.len(), 1);
    assert_eq!(played[0].bytes(), tone_wav().as_slice());
}

#[tokio::test]
async fn a_first_run_with_a_refused_address_saves_nothing() {
    let mut run = Run::new().in_a_terminal("my-host/path\n\n");

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    assert!(error.to_string().contains("path"), "{error:#}");
    assert!(!run.config_path().exists());
}

#[tokio::test]
async fn a_config_on_disk_sends_the_text_with_its_model_and_voice() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut run = Run::new().with_config(&listener.config());

    run.speak(text(SPOKEN)).await.unwrap();

    assert_eq!(
        listener.received()[0].json(),
        json!({"model": "chatterbox-turbo", "voice": "sj-clip-001.wav", "input": SPOKEN})
    );
    assert_eq!(run.played.borrow().len(), 1);
}

#[tokio::test]
async fn model_and_voice_overrides_reach_the_request_and_leave_the_file_alone() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut run = Run::new().with_config(&listener.config());
    let before = run.config_text();

    run.speak(SpeakArgs {
        text: Some(SPOKEN.to_owned()),
        model: Some("other-model".to_owned()),
        voice: Some("other.wav".to_owned()),
    })
    .await
    .unwrap();

    assert_eq!(
        listener.received()[0].json(),
        json!({"model": "other-model", "voice": "other.wav", "input": SPOKEN})
    );
    assert_eq!(run.config_text(), before);
}

#[tokio::test]
async fn text_piped_on_stdin_reaches_the_server_as_input() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut run = Run::new()
        .with_config(&listener.config())
        .piping("Piped \"quoted\" text\n");

    run.speak(SpeakArgs::default()).await.unwrap();

    assert_eq!(
        listener.received()[0].json()["input"],
        "Piped \"quoted\" text"
    );
}

#[tokio::test]
async fn a_server_error_carries_its_detail_and_plays_nothing() {
    let listener = Listener::start(Reply::json(
        404,
        &json!({"detail": "Voice file 'nope.wav' not found."}),
    ));
    let mut run = Run::new().with_config(&listener.config());

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    assert!(
        error
            .to_string()
            .contains("Voice file 'nope.wav' not found."),
        "{error:#}"
    );
    assert!(run.played.borrow().is_empty());
}

#[tokio::test]
async fn an_answer_that_is_not_audio_names_its_content_type_size_and_the_formats() {
    let listener = Listener::start(Reply::new(200, "text/html", "<html>not audio</html>"));
    let mut run = Run::new().with_config(&listener.config());

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains("content type text/html, 22 bytes"),
        "{message}"
    );
    assert!(message.contains("WAV, MP3, FLAC, Ogg Vorbis"), "{message}");
    assert!(run.played.borrow().is_empty());
}

#[tokio::test]
async fn a_playback_failure_is_returned() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut run = Run::new().with_config(&listener.config());
    run.play_fails = true;

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    assert_eq!(error.to_string(), "No sound output device could be opened");
}

#[tokio::test]
async fn an_unreadable_config_names_its_path_and_sends_nothing() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut run = Run::new().with_config(&listener.config());
    std::fs::write(run.config_path(), "{ not json").unwrap();

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    assert!(format!("{error:#}").contains(&run.config_path().display().to_string()));
    assert!(listener.received().is_empty());
    assert_eq!(run.config_text(), "{ not json");
}

#[tokio::test]
async fn a_config_that_does_not_validate_sends_nothing() {
    let listener = Listener::start(Reply::audio(tone_wav()));
    let mut config = listener.config();
    config.request_method = "GET".to_owned();
    let mut run = Run::new().with_config(&config);

    let error = run.speak(text(SPOKEN)).await.unwrap_err();

    assert!(error.to_string().contains("POST or PUT"), "{error:#}");
    assert!(listener.received().is_empty());
}

#[tokio::test]
#[ignore = "speaks through a real server and the real output device; set SPEAK_LIVE_URL to the server's speech URL"]
async fn a_live_server_is_heard() {
    let url = std::env::var("SPEAK_LIVE_URL").expect("SPEAK_LIVE_URL is set");
    let config = SpeakConfig {
        url,
        ..SpeakConfig::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    config.save(&config_path).unwrap();
    let deps = Deps {
        config_path,
        stdin_terminal: true,
        stderr_terminal: true,
        stdin: Cursor::new(Vec::new()),
        prompt: Vec::new(),
        play: |clip: Clip| {
            let mut speaker = common_audio::Speaker::open()?;
            speaker.play(&clip)?;
            speaker.wait();
            Ok(())
        },
    };

    run(text("Hello. [chuckle] The live check works."), deps)
        .await
        .unwrap();
}
