use super::*;

const NAMESPACE: &str = "orders.servicebus.windows.net";
const MARKER: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKey=marker-7731";

fn typed(text: &str) -> SecretSource {
    let mut secret = SecretSource::typed();
    secret.edit_mut().replace_text(text);
    secret
}

fn credential_form() -> ConnectForm {
    ConnectForm {
        auth: AuthChoice::Credential,
        namespace: TextEdit::new(NAMESPACE),
        event_hub: TextEdit::new("orders"),
        ..ConnectForm::default()
    }
}

fn recalled(auth: AuthMethodConfig) -> ToolConfig {
    let mut config = ToolConfig::new(TargetConfig {
        namespace: Some(NAMESPACE.to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: "audit".to_owned(),
        auth,
    });
    config.capture.prefetch = Some(64);
    config
}

#[test]
fn an_empty_connection_string_form_asks_only_for_the_connection_string() {
    let issues = ConnectForm::default().issues();

    assert_eq!(
        issues,
        FieldIssues {
            connection_string: Some("Enter the connection string".to_owned()),
            ..FieldIssues::default()
        }
    );
}

#[test]
fn credential_auth_requires_the_namespace_and_event_hub() {
    let form = ConnectForm {
        auth: AuthChoice::Credential,
        ..ConnectForm::default()
    };

    let issues = form.issues();

    assert!(issues.namespace.is_some());
    assert!(issues.event_hub.is_some());
    assert!(issues.connection_string.is_none());
}

#[test]
fn a_typed_namespace_goes_through_the_engine_validator() {
    let form = ConnectForm {
        connection_string: typed(MARKER),
        namespace: TextEdit::new("orders.example.com"),
        ..ConnectForm::default()
    };

    let issue = form.issues().namespace.unwrap();

    assert!(issue.contains("required suffixes"), "{issue}");
}

#[test]
fn an_invalid_event_hub_name_is_not_echoed() {
    let form = ConnectForm {
        event_hub: TextEdit::new("SharedAccessKey=marker"),
        ..credential_form()
    };

    let issue = form.issues().event_hub.unwrap();

    assert!(!issue.contains("marker"), "{issue}");
}

#[test]
fn key_vault_auth_requires_the_vault_url_and_secret_name() {
    let form = ConnectForm {
        auth: AuthChoice::KeyVault,
        ..ConnectForm::default()
    };

    let issues = form.issues();

    assert!(issues.vault_url.is_some());
    assert!(issues.secret_name.is_some());
    assert!(issues.namespace.is_none());
}

#[test]
fn a_client_secret_credential_requires_its_ids_and_secret() {
    let form = ConnectForm {
        credential: CredentialChoice::ClientSecret,
        ..credential_form()
    };

    let issues = form.issues();

    assert!(issues.tenant_id.is_some());
    assert!(issues.client_id.is_some());
    assert!(issues.client_secret.is_some());
}

#[test]
fn a_stored_client_secret_satisfies_the_form() {
    let form = ConnectForm {
        credential: CredentialChoice::ClientSecret,
        tenant_id: TextEdit::new("tenant"),
        client_id: TextEdit::new("client"),
        client_secret: SecretSource::stored(),
        ..credential_form()
    };

    assert!(form.issues().is_empty(), "{:?}", form.issues());
}

#[test]
fn the_message_folder_is_required_only_when_disk_write_is_on() {
    let off = ConnectForm {
        received_msg_path: TextEdit::new(""),
        ..credential_form()
    };
    let immediate = ConnectForm {
        disk_write: DiskWriteMode::Immediate,
        received_msg_path: TextEdit::new(""),
        ..credential_form()
    };

    assert!(off.issues().received_msg_path.is_none());
    assert!(immediate.issues().received_msg_path.is_some());
}

#[test]
fn a_typed_connection_string_maps_to_a_stored_reference_plus_the_given_secret() {
    let form = ConnectForm {
        connection_string: typed(MARKER),
        ..ConnectForm::default()
    };

    let (config, sensitive) = form.to_config();

    assert_eq!(
        config.target.auth,
        AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb
        }
    );
    assert_eq!(sensitive.connection_string.unwrap().expose(), MARKER);
    assert!(sensitive.client_secret.is_none());
}

#[test]
fn storage_fields_map_to_the_storage_config() {
    let form = ConnectForm {
        data_folder: "captures".to_owned(),
        disk_write: DiskWriteMode::AtEnd,
        received_msg_path: TextEdit::new("dumps"),
        ..credential_form()
    };

    let (config, _) = form.to_config();

    assert_eq!(config.storage.base_data_folder, "captures");
    assert_eq!(config.storage.disk_write.mode, DiskWriteMode::AtEnd);
    assert_eq!(config.storage.disk_write.received_msg_path, "dumps");
    assert_eq!(form.data_paths(), DataPaths::new("captures"));
}

#[test]
fn a_recalled_connection_string_config_marks_the_secret_stored() {
    let config = recalled(AuthMethodConfig::ConnectionString {
        stored: StoredSecret::LocalDb,
    });

    let (form, loaded) = ConnectForm::from_config(&config);
    let (rebuilt, sensitive) = form.to_config();

    assert!(form.connection_string.is_stored());
    assert!(sensitive.connection_string.is_none());
    assert_eq!(rebuilt, config);
    assert!(loaded.iter().all(|name| !name.contains(NAMESPACE)));
}

#[test]
fn a_recalled_client_secret_config_fills_the_ids_and_marks_the_secret_stored() {
    let config = recalled(AuthMethodConfig::KeyVaultConnectionString {
        vault_url: "https://vault.vault.azure.net".to_owned(),
        secret_name: "orders-cs".to_owned(),
        credential: CredentialKindConfig::ClientSecret {
            tenant_id: "tenant".to_owned(),
            client_id: "client".to_owned(),
        },
    });

    let (form, _) = ConnectForm::from_config(&config);

    assert_eq!(form.auth, AuthChoice::KeyVault);
    assert_eq!(form.credential, CredentialChoice::ClientSecret);
    assert!(form.client_secret.is_stored());
    assert!(form.issues().is_empty(), "{:?}", form.issues());
    assert_eq!(form.to_config().0, config);
}

#[test]
fn switching_the_auth_method_hides_a_revealed_secret() {
    let mut form = ConnectForm {
        connection_string: typed(MARKER),
        ..ConnectForm::default()
    };
    form.connection_string.edit_mut().set_revealed(true);

    form.set_auth(AuthChoice::Credential);

    assert!(!form.connection_string.edit().is_revealed());
}

#[test]
fn clearing_secrets_empties_the_typed_fields() {
    let mut form = ConnectForm {
        connection_string: typed(MARKER),
        client_secret: typed("client-marker"),
        ..ConnectForm::default()
    };

    form.clear_secrets();

    assert!(form.connection_string.edit().text.is_empty());
    assert!(form.client_secret.edit().text.is_empty());
}

#[test]
fn debug_output_holds_no_field_text() {
    let form = ConnectForm {
        connection_string: typed(MARKER),
        namespace: TextEdit::new(NAMESPACE),
        ..ConnectForm::default()
    };

    let printed = format!("{form:?}");

    assert!(!printed.contains("marker"), "{printed}");
    assert!(!printed.contains(NAMESPACE), "{printed}");
}
