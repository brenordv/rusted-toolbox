use super::*;
use rstest::rstest;
use serde_json::json;
use tempfile::tempdir;

fn config_at(url: &str) -> SpeakConfig {
    SpeakConfig {
        url: url.to_owned(),
        ..SpeakConfig::default()
    }
}

#[test]
fn for_server_builds_the_speech_url_with_the_default_settings() {
    let config = SpeakConfig::for_server("my-host", 10157).unwrap();

    assert_eq!(config.url, "http://my-host:10157/v1/audio/speech");
    assert_eq!(config.request_method, "POST");
    assert_eq!(config.model(), Some("chatterbox-turbo"));
    assert_eq!(config.voice(), Some("sj-clip-001.wav"));
    assert_eq!(config.supported_vocal_reactions, DEFAULT_REACTIONS);
}

#[rstest]
#[case::https("https://my-host", "https://my-host:8443/v1/audio/speech")]
#[case::http("http://my-host", "http://my-host:8443/v1/audio/speech")]
#[case::trailing_slash("http://my-host/", "http://my-host:8443/v1/audio/speech")]
#[case::surrounding_spaces("  my-host \r\n", "http://my-host:8443/v1/audio/speech")]
#[case::ip_address("192.0.2.7", "http://192.0.2.7:8443/v1/audio/speech")]
fn for_server_keeps_a_given_scheme(#[case] address: &str, #[case] url: &str) {
    let config = SpeakConfig::for_server(address, 8443).unwrap();

    assert_eq!(config.url, url);
}

#[rstest]
#[case::blank("   ", "blank")]
#[case::path("my-host/v1", "path")]
#[case::query("my-host?x=1", "query")]
#[case::fragment("my-host#top", "path")]
#[case::port("my-host:8080", "port")]
#[case::default_port("http://my-host:80", "port")]
#[case::user_name("someone@my-host", "user name")]
#[case::password("http://someone:pw@my-host", "user name")]
#[case::spaces("my host", "spaces")]
#[case::control_character("my\u{7}host", "control characters")]
#[case::other_scheme("ftp://my-host", "http://")]
#[case::not_an_address("http://", "not valid")]
fn for_server_refuses_more_than_a_host_with_the_reason(
    #[case] address: &str,
    #[case] reason: &str,
) {
    let error = SpeakConfig::for_server(address, 10157).unwrap_err();

    assert!(error.to_string().contains(reason), "{error:#}");
}

#[tokio::test]
async fn a_saved_config_loads_back_equal_and_holds_no_input() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tool-speak").join("config.json");
    let config = SpeakConfig::for_server("my-host", 10157).unwrap();

    config.save(&path).unwrap();

    assert_eq!(load(&path).await.unwrap(), Some(config));
    assert!(!std::fs::read_to_string(&path).unwrap().contains("input"));
}

#[tokio::test]
async fn loading_a_missing_file_is_none() {
    let dir = tempdir().unwrap();

    let loaded = load(&dir.path().join("config.json")).await.unwrap();

    assert_eq!(loaded, None);
}

#[tokio::test]
async fn loading_invalid_json_names_the_path_and_leaves_the_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, "{ not json").unwrap();

    let error = load(&path).await.unwrap_err();

    assert!(format!("{error:#}").contains(&path.display().to_string()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
}

#[tokio::test]
async fn loading_a_folder_in_place_of_the_file_is_an_error() {
    let dir = tempdir().unwrap();

    let error = load(dir.path()).await.unwrap_err();

    assert!(format!("{error:#}").contains(&dir.path().display().to_string()));
}

#[tokio::test]
async fn extra_payload_fields_survive_a_load_and_save() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut config = SpeakConfig::for_server("my-host", 10157).unwrap();
    config.payload.insert("speed".to_owned(), json!(1.25));
    config.save(&path).unwrap();

    let loaded = load(&path).await.unwrap().unwrap();
    loaded.save(&path).unwrap();

    let reloaded = load(&path).await.unwrap().unwrap();
    assert_eq!(reloaded.payload.get("speed"), Some(&json!(1.25)));
}

#[test]
fn saving_into_a_path_that_cannot_be_written_names_the_path() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "a file").unwrap();
    let path = blocker.join("config.json");

    let error = SpeakConfig::default().save(&path).unwrap_err();

    assert!(format!("{error:#}").contains(&path.display().to_string()));
}

#[test]
fn a_config_for_a_server_validates() {
    let config = SpeakConfig::for_server("my-host", 10157).unwrap();

    assert!(config.validate().is_ok());
}

#[rstest]
#[case::unset("", "not set")]
#[case::not_a_url("my-host", "not a valid URL")]
#[case::other_scheme("ftp://my-host/v1/audio/speech", "http:// or https://")]
#[case::empty_host("http://", "not a valid URL")]
#[case::user_name("http://someone@my-host/v1/audio/speech", "user name or password")]
#[case::password(
    "http://someone:hunter2@my-host/v1/audio/speech",
    "user name or password"
)]
fn validate_refuses_a_url_it_cannot_use(#[case] url: &str, #[case] reason: &str) {
    let error = config_at(url).validate().unwrap_err();

    assert!(error.to_string().contains(reason), "{error:#}");
}

#[test]
fn a_refused_url_is_not_repeated_in_the_error() {
    let error = config_at("http://someone:hunter2@my-host/v1/audio/speech")
        .validate()
        .unwrap_err();

    let text = format!("{error:#}");
    assert!(!text.contains("hunter2"), "{text}");
    assert!(!text.contains("someone"), "{text}");
}

#[rstest]
#[case::get("GET")]
#[case::lowercase("post")]
#[case::blank("")]
fn validate_refuses_a_method_other_than_post_or_put(#[case] method: &str) {
    let mut config = SpeakConfig::for_server("my-host", 10157).unwrap();
    config.request_method = method.to_owned();

    let error = config.validate().unwrap_err();

    assert!(error.to_string().contains("POST or PUT"), "{error:#}");
}

#[test]
fn put_is_a_valid_method() {
    let mut config = SpeakConfig::for_server("my-host", 10157).unwrap();
    config.request_method = "PUT".to_owned();

    assert_eq!(config.method().unwrap(), Method::PUT);
}

#[rstest]
#[case::blank_model("model", json!("  "))]
#[case::missing_model("model", Value::Null)]
#[case::numeric_model("model", json!(3))]
#[case::blank_voice("voice", json!(""))]
fn validate_refuses_a_model_or_voice_that_is_not_a_text(#[case] key: &str, #[case] value: Value) {
    let mut config = SpeakConfig::for_server("my-host", 10157).unwrap();
    config.payload.insert(key.to_owned(), value);

    let error = config.validate().unwrap_err();

    assert!(error.to_string().contains(key), "{error:#}");
}

#[test]
fn the_request_body_is_the_payload_with_the_text_as_input() {
    let mut config = SpeakConfig::for_server("my-host", 10157).unwrap();
    config.payload.insert("input".to_owned(), json!("stale"));
    config.set_model("other-model");
    config.set_voice("other.wav");

    let body = config.request_body("Hello");

    assert_eq!(
        Value::Object(body),
        json!({"model": "other-model", "voice": "other.wav", "input": "Hello"})
    );
}

#[test]
fn the_default_path_is_the_tools_folder_under_the_shared_root() {
    let path = default_config_path();

    assert!(path.ends_with(".rusted-toolbox/tool-speak/config.json"));
}
