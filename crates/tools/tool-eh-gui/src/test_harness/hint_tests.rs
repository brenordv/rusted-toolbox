use super::builders::{open_sheet, state};
use super::profile_tests::{named_string_config, recalled_from, texts_within};
use super::sweep_tests::sheet;
use super::{Harness, rect};
use crate::design::tokens::{TYPE_MONO, WINDOW_MIN, WINDOW_SIZE};
use crate::form::{AuthChoice, CredentialChoice, SecretSource};
use crate::state::{RECALLED_FIELD, SheetPurpose};
use crate::views::{CONNECTION_STRING_EXAMPLE_KEY, NEW_PROFILE_KEY, hints};

/// Every line the sheet can show under a sign-in field or picker.
fn every_hint() -> [&'static str; 20] {
    [
        hints::sign_in(AuthChoice::ConnectionString),
        hints::sign_in(AuthChoice::Credential),
        hints::sign_in(AuthChoice::KeyVault),
        hints::CONNECTION_STRING,
        hints::CONNECTION_STRING_EXAMPLE,
        hints::VAULT_URL,
        hints::SECRET_NAME,
        hints::namespace(AuthChoice::ConnectionString),
        hints::namespace(AuthChoice::Credential),
        hints::namespace(AuthChoice::KeyVault),
        hints::event_hub(AuthChoice::ConnectionString),
        hints::event_hub(AuthChoice::Credential),
        hints::event_hub(AuthChoice::KeyVault),
        hints::credential(CredentialChoice::DeveloperTools),
        hints::credential(CredentialChoice::ManagedIdentity),
        hints::credential(CredentialChoice::ClientSecret),
        hints::TENANT_ID,
        hints::CLIENT_ID,
        hints::CLIENT_SECRET,
        hints::CONSUMER_GROUP,
    ]
}

/// The hint lines the open sheet shows, top to bottom.
fn hints_shown(h: &Harness) -> Vec<String> {
    let every = every_hint();
    texts_within(h, "connect-form")
        .into_iter()
        .filter(|text| every.contains(&text.as_str()))
        .collect()
}

/// The rect of the first mounted element whose text is `text`.
fn text_rect(h: &Harness, text: &str) -> [f32; 4] {
    h.rt.elements()
        .into_iter()
        .find(|element| !element.exiting && element.text.as_deref() == Some(text))
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("no element shows {text:?}"))
}

/// The rect of the first mounted element labeled `label`.
fn labeled_rect(h: &Harness, label: &str) -> [f32; 4] {
    h.rt.elements()
        .into_iter()
        .find(|element| !element.exiting && element.label.as_deref() == Some(label))
        .map(|element| element.rect)
        .unwrap_or_else(|| panic!("no element labeled {label:?}"))
}

fn credential_sheet(credential: CredentialChoice) -> Harness {
    let mut s = open_sheet(AuthChoice::Credential);
    s.form.credential = credential;
    Harness::new(s, WINDOW_SIZE)
}

#[rstest::rstest]
#[case::connection_string(AuthChoice::ConnectionString, vec![
    hints::sign_in(AuthChoice::ConnectionString),
    hints::CONNECTION_STRING,
    hints::CONNECTION_STRING_EXAMPLE,
    hints::namespace(AuthChoice::ConnectionString),
    hints::event_hub(AuthChoice::ConnectionString),
    hints::CONSUMER_GROUP,
])]
#[case::credential(AuthChoice::Credential, vec![
    hints::sign_in(AuthChoice::Credential),
    hints::namespace(AuthChoice::Credential),
    hints::event_hub(AuthChoice::Credential),
    hints::credential(CredentialChoice::ClientSecret),
    hints::TENANT_ID,
    hints::CLIENT_ID,
    hints::CLIENT_SECRET,
    hints::CONSUMER_GROUP,
])]
#[case::key_vault(AuthChoice::KeyVault, vec![
    hints::sign_in(AuthChoice::KeyVault),
    hints::VAULT_URL,
    hints::SECRET_NAME,
    hints::namespace(AuthChoice::KeyVault),
    hints::event_hub(AuthChoice::KeyVault),
    hints::credential(CredentialChoice::ClientSecret),
    hints::TENANT_ID,
    hints::CLIENT_ID,
    hints::CLIENT_SECRET,
    hints::CONSUMER_GROUP,
])]
fn each_sign_in_method_shows_the_hints_of_its_own_fields_in_order(
    #[case] auth: AuthChoice,
    #[case] expected: Vec<&str>,
) {
    let h = sheet(WINDOW_SIZE, auth);

    let shown = hints_shown(&h);

    assert_eq!(shown, expected);
}

#[rstest::rstest]
#[case::developer_tools(CredentialChoice::DeveloperTools, vec![
    hints::sign_in(AuthChoice::Credential),
    hints::namespace(AuthChoice::Credential),
    hints::event_hub(AuthChoice::Credential),
    hints::credential(CredentialChoice::DeveloperTools),
    hints::CONSUMER_GROUP,
])]
#[case::managed_identity(CredentialChoice::ManagedIdentity, vec![
    hints::sign_in(AuthChoice::Credential),
    hints::namespace(AuthChoice::Credential),
    hints::event_hub(AuthChoice::Credential),
    hints::credential(CredentialChoice::ManagedIdentity),
    hints::CONSUMER_GROUP,
])]
#[case::client_secret(CredentialChoice::ClientSecret, vec![
    hints::sign_in(AuthChoice::Credential),
    hints::namespace(AuthChoice::Credential),
    hints::event_hub(AuthChoice::Credential),
    hints::credential(CredentialChoice::ClientSecret),
    hints::TENANT_ID,
    hints::CLIENT_ID,
    hints::CLIENT_SECRET,
    hints::CONSUMER_GROUP,
])]
fn each_credential_shows_its_caption_and_only_client_secret_shows_the_app_registration_hints(
    #[case] credential: CredentialChoice,
    #[case] expected: Vec<&str>,
) {
    let h = credential_sheet(credential);

    let shown = hints_shown(&h);

    assert_eq!(shown, expected);
}

#[test]
fn a_stored_connection_string_shows_saved_in_local_store_with_no_hint_or_example() {
    let mut s = state();
    s.form.set_auth(AuthChoice::ConnectionString);
    s.form.connection_string = SecretSource::stored();
    s.sheet = Some(SheetPurpose::Connect);
    let h = Harness::new(s, WINDOW_SIZE);

    let shown = hints_shown(&h);

    assert!(h.has_text("Saved in local store"));
    assert!(!h.has_key(CONNECTION_STRING_EXAMPLE_KEY));
    assert_eq!(
        shown,
        vec![
            hints::sign_in(AuthChoice::ConnectionString),
            hints::namespace(AuthChoice::ConnectionString),
            hints::event_hub(AuthChoice::ConnectionString),
            hints::CONSUMER_GROUP,
        ]
    );
}

#[rstest::rstest]
#[case::text_field(AuthChoice::KeyVault, "Enter the vault URL", hints::VAULT_URL)]
#[case::secret_field(
    AuthChoice::ConnectionString,
    "Enter the connection string",
    hints::CONNECTION_STRING
)]
fn a_field_with_an_issue_shows_the_issue_and_then_its_hint(
    #[case] auth: AuthChoice,
    #[case] issue: &str,
    #[case] hint: &str,
) {
    let mut s = open_sheet(auth);
    s.show_issues = true;
    let h = Harness::new(s, WINDOW_SIZE);

    let issue_top = text_rect(&h, issue)[1];
    let hint_top = text_rect(&h, hint)[1];

    assert!(
        issue_top < hint_top,
        "the issue at {issue_top}, the hint at {hint_top}"
    );
}

#[test]
fn a_recalled_namespace_shows_the_field_then_the_recall_mark_then_the_hint() {
    let mut h = recalled_from(named_string_config(), WINDOW_SIZE);

    h.press(NEW_PROFILE_KEY);
    let field_top = labeled_rect(&h, "Namespace (optional)")[1];
    let mark_top = text_rect(&h, RECALLED_FIELD)[1];
    let hint_top = text_rect(&h, hints::namespace(AuthChoice::ConnectionString))[1];

    assert!(
        field_top < mark_top && mark_top < hint_top,
        "the field at {field_top}, the mark at {mark_top}, the hint at {hint_top}"
    );
}

#[test]
fn the_connection_string_example_breaks_inside_the_sheet_in_the_smallest_window() {
    let h = sheet(WINDOW_MIN, AuthChoice::ConnectionString);

    let [panel_left, _, panel_width, _] = rect(&h.rt, "connect-dialog");
    let [left, _, width, height] = rect(&h.rt, CONNECTION_STRING_EXAMPLE_KEY);

    assert!(
        height > TYPE_MONO.line_px(),
        "the example is one line, {height} tall"
    );
    assert!(
        left >= panel_left - 0.5 && left + width <= panel_left + panel_width + 0.5,
        "the example spans {left}..{} in a sheet spanning {panel_left}..{}",
        left + width,
        panel_left + panel_width
    );
}
