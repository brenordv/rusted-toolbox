use super::*;
use rstest::rstest;
use serde_json::json;
use std::fs;
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

    let text = format!("{error:#}");
    assert!(text.contains(&path.display().to_string()), "{text}");
    assert!(text.contains("failed to create folder"), "{text}");
    assert_eq!(std::fs::read(&blocker).unwrap(), b"a file");
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

fn parts(server: &str, endpoint: &str) -> UrlParts {
    UrlParts {
        server: server.to_owned(),
        endpoint: endpoint.to_owned(),
    }
}

#[rstest]
#[case::as_the_first_run_builds_it(
    "http://my-host:10157/v1/audio/speech",
    "http://my-host:10157",
    "/v1/audio/speech"
)]
#[case::https("https://my-host:8443/speak", "https://my-host:8443", "/speak")]
#[case::no_port("http://my-host/v1/audio/speech", "http://my-host", "/v1/audio/speech")]
#[case::default_port("http://my-host:80/x", "http://my-host:80", "/x")]
#[case::ipv6("http://[::1]:10157/x", "http://[::1]:10157", "/x")]
#[case::idn_host("http://bücher.example/x", "http://bücher.example", "/x")]
#[case::query("http://my-host/x?lang=en", "http://my-host", "/x?lang=en")]
#[case::query_and_no_path("http://my-host?lang=en", "http://my-host", "?lang=en")]
#[case::fragment("http://my-host/x#top", "http://my-host", "/x#top")]
#[case::no_path("http://my-host", "http://my-host", "")]
#[case::root_path("http://my-host/", "http://my-host", "/")]
#[case::surrounding_spaces("  http://my-host/x \n", "http://my-host", "/x")]
fn split_url_cuts_a_url_after_its_host_and_port(
    #[case] url: &str,
    #[case] server: &str,
    #[case] endpoint: &str,
) {
    assert_eq!(split_url(url), parts(server, endpoint));
}

#[rstest]
#[case::blank("")]
#[case::only_spaces("   ")]
fn split_url_of_a_blank_url_is_an_empty_server_and_the_speech_path(#[case] url: &str) {
    assert_eq!(split_url(url), parts("", SPEECH_PATH));
}

#[rstest]
#[case::a_host("my-host")]
#[case::a_host_port_and_path("my-host:10157/x")]
fn split_url_of_text_with_no_scheme_is_all_server(#[case] text: &str) {
    assert_eq!(split_url(text), parts(text, ""));
}

#[rstest]
#[case::scheme_and_port(
    "http://my-host:10157",
    "/v1/audio/speech",
    "http://my-host:10157/v1/audio/speech"
)]
#[case::no_scheme(
    "my-host:10157",
    "/v1/audio/speech",
    "http://my-host:10157/v1/audio/speech"
)]
#[case::trailing_slash(
    "http://my-host:10157/",
    "/v1/audio/speech",
    "http://my-host:10157/v1/audio/speech"
)]
#[case::https("https://my-host", "/speak", "https://my-host/speak")]
#[case::path_prefix(
    "http://my-host/api",
    "/v1/audio/speech",
    "http://my-host/api/v1/audio/speech"
)]
#[case::no_leading_slash("http://my-host", "v1/audio/speech", "http://my-host/v1/audio/speech")]
#[case::endpoint_is_a_query("http://my-host", "?lang=en", "http://my-host?lang=en")]
#[case::blank_endpoint("http://my-host:10157", "", "http://my-host:10157")]
#[case::blank_endpoint_keeps_the_slash("http://my-host/", "  ", "http://my-host/")]
#[case::surrounding_spaces("  http://my-host \n", " /speak ", "http://my-host/speak")]
fn join_url_puts_the_endpoint_after_the_server(
    #[case] server: &str,
    #[case] endpoint: &str,
    #[case] url: &str,
) {
    assert_eq!(join_url(server, endpoint).unwrap(), url);
}

#[rstest]
#[case::blank_server("  ", "/v1/audio/speech", "is not set")]
#[case::query_in_server("http://my-host?x=1", "/v1/audio/speech", "? or a #")]
#[case::fragment_in_server("http://my-host#top", "/v1/audio/speech", "? or a #")]
#[case::other_scheme("ftp://my-host", "/v1/audio/speech", "http:// or https://")]
#[case::user_info(
    "http://someone:hunter2@my-host",
    "/v1/audio/speech",
    "user name or password"
)]
#[case::space_in_the_host("http://my host", "/v1/audio/speech", "not a valid URL")]
#[case::no_host("http://", "", "not a valid URL")]
#[case::no_host_with_an_endpoint("http://", "/v1/audio/speech", "not a valid URL")]
#[case::no_host_and_a_backslash("http://\\", "/v1/audio/speech", "not a valid URL")]
#[case::ends_in_an_at_sign("http://my-host@", "/v1/audio/speech", "not a valid URL")]
fn join_url_refuses_a_server_it_cannot_use(
    #[case] server: &str,
    #[case] endpoint: &str,
    #[case] reason: &str,
) {
    let error = join_url(server, endpoint).unwrap_err();

    assert!(error.to_string().contains(reason), "{error:#}");
}

#[rstest]
#[case::as_the_first_run_builds_it("http://my-host:10157/v1/audio/speech")]
#[case::https("https://my-host:8443/speak")]
#[case::default_port("http://my-host:80/x")]
#[case::ipv6("http://[::1]:10157/x")]
#[case::idn_host("http://bücher.example/x")]
#[case::query("http://my-host/x?lang=en")]
#[case::query_and_no_path("http://my-host?lang=en")]
#[case::fragment("http://my-host/x#top")]
#[case::no_path("http://my-host")]
#[case::root_path("http://my-host/")]
#[case::doubled_slash("http://my-host//x")]
fn a_url_split_then_joined_is_the_same_text(#[case] url: &str) {
    let UrlParts { server, endpoint } = split_url(url);

    assert_eq!(join_url(&server, &endpoint).unwrap(), url);
}

#[test]
fn the_url_for_server_builds_survives_a_split_and_a_join() {
    let url = SpeakConfig::for_server("my-host", 10157).unwrap().url;
    let UrlParts { server, endpoint } = split_url(&url);

    assert_eq!(join_url(&server, &endpoint).unwrap(), url);
}

#[rstest]
#[case::password("http://someone:hunter2@my-host", "/private-path")]
#[case::query("http://my-host?token=hunter2", "/private-path")]
#[case::other_scheme("ftp://hunter2.example", "/private-path")]
#[case::not_a_host("http://hunter2 host", "/private-path")]
fn a_refused_server_and_its_endpoint_are_not_repeated_in_the_error(
    #[case] server: &str,
    #[case] endpoint: &str,
) {
    let error = join_url(server, endpoint).unwrap_err();

    let text = format!("{error:#}");
    assert!(!text.contains("hunter2"), "{text}");
    assert!(!text.contains("private-path"), "{text}");
}

#[rstest]
#[case::at_sign("@evil/x")]
#[case::backslashes("\\\\evil")]
#[case::dot_segments("/../x")]
#[case::encoded_slashes("%2f%2fevil")]
#[case::doubled_slash("//evil/x")]
fn an_endpoint_cannot_change_the_scheme_host_or_port(#[case] endpoint: &str) {
    let joined = join_url("https://my-host:8443", endpoint).unwrap();

    let url = Url::parse(&joined).unwrap();
    assert_eq!(
        (url.scheme(), url.host_str(), url.port()),
        ("https", Some("my-host"), Some(8443))
    );
}

fn speaks_file(root: &Path) -> PathBuf {
    root.join("speak").join("config.json")
}

fn old_file(root: &Path) -> PathBuf {
    root.join("tool-speak").join("config.json")
}

#[tokio::test]
async fn settings_only_in_the_old_folder_move_to_speaks_folder_and_load_from_there() {
    let root = tempdir().unwrap();
    let saved = config_at("http://old-host:10157/v1/audio/speech");
    saved.save(&old_file(root.path())).unwrap();

    let location = locate_config(root.path());

    assert!(matches!(location.adoption, Adoption::Moved { .. }));
    assert_eq!(location.path(), speaks_file(root.path()));
    assert_eq!(load(&location.path()).await.unwrap(), Some(saved));
    assert!(!old_file(root.path()).exists());
}

#[test]
fn with_no_settings_anywhere_the_file_goes_in_speaks_folder() {
    let root = tempdir().unwrap();

    let location = locate_config(root.path());

    assert!(matches!(location.adoption, Adoption::Nothing));
    assert_eq!(location.path(), speaks_file(root.path()));
    assert_eq!(location.target, speaks_file(root.path()));
}

#[tokio::test]
async fn settings_that_cannot_move_are_read_from_the_old_folder() {
    let root = tempdir().unwrap();
    let saved = config_at("http://old-host:10157/v1/audio/speech");
    saved.save(&old_file(root.path())).unwrap();
    fs::write(
        root.path().join("speak"),
        b"a file where speak's folder goes",
    )
    .unwrap();

    let location = locate_config(root.path());

    assert!(matches!(location.adoption, Adoption::KeptLegacy { .. }));
    assert_eq!(location.path(), old_file(root.path()));
    assert_eq!(load(&location.path()).await.unwrap(), Some(saved));
}

#[cfg(windows)]
#[tokio::test]
async fn settings_held_open_in_the_old_folder_are_used_and_saved_in_speaks_folder() {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    let root = tempdir().unwrap();
    let original = config_at("http://old-host:1/v1/audio/speech");
    original.save(&old_file(root.path())).unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE)
        .open(old_file(root.path()))
        .unwrap();

    let location = locate_config(root.path());
    drop(held);
    let edited = config_at("http://new-host:2/v1/audio/speech");
    edited.save(&location.path()).unwrap();
    let next_start = locate_config(root.path());

    assert!(matches!(location.adoption, Adoption::OldNameLeft { .. }));
    assert_eq!(location.path(), speaks_file(root.path()));
    assert!(matches!(next_start.adoption, Adoption::BothPresent { .. }));
    assert_eq!(load(&next_start.path()).await.unwrap(), Some(edited));
    assert_eq!(load(&old_file(root.path())).await.unwrap(), Some(original));
}

#[tokio::test]
async fn settings_in_both_folders_are_read_from_speaks_folder() {
    let root = tempdir().unwrap();
    config_at("http://old-host:1/v1/audio/speech")
        .save(&old_file(root.path()))
        .unwrap();
    let current = config_at("http://new-host:2/v1/audio/speech");
    current.save(&speaks_file(root.path())).unwrap();

    let location = locate_config(root.path());

    assert!(matches!(location.adoption, Adoption::BothPresent { .. }));
    assert_eq!(load(&location.path()).await.unwrap(), Some(current));
    assert!(old_file(root.path()).exists());
}

#[cfg(unix)]
#[test]
fn save_creates_a_missing_folder_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir().unwrap();
    let path = speaks_file(root.path());

    SpeakConfig::default().save(&path).unwrap();

    let mode = fs::metadata(path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0);
}
