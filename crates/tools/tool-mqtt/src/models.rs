/// Broker credentials, always carried as a complete pair: the CLI resolver
/// only builds one when both `--username` and `--password` are present, so a
/// one-sided pair is unrepresentable past argument parsing.
///
/// Deliberately carries no `Debug` derive: a derived formatter would print the
/// password anywhere the config is `{:?}`-formatted.
pub struct MqttCredentials {
    pub username: String,
    pub password: String,
}

pub struct MqttConfig {
    pub command: MqttCommand,
    pub host: String,
    pub port: u16,
    pub topic: String,
    pub message: Option<String>,
    pub credentials: Option<MqttCredentials>,
}

impl MqttConfig {
    pub fn is_anonymous(&self) -> bool {
        self.credentials.is_none()
    }
}

pub enum MqttCommand {
    Read,
    Post,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with_credentials(credentials: Option<MqttCredentials>) -> MqttConfig {
        MqttConfig {
            command: MqttCommand::Read,
            host: "localhost".to_string(),
            port: 1883,
            topic: "sensors/temp".to_string(),
            message: None,
            credentials,
        }
    }

    #[test]
    fn is_anonymous_true_when_no_credentials() {
        assert!(args_with_credentials(None).is_anonymous());
    }

    #[test]
    fn is_anonymous_false_when_credentials_present() {
        let credentials = MqttCredentials {
            username: "user".to_string(),
            password: "pass".to_string(),
        };

        assert!(!args_with_credentials(Some(credentials)).is_anonymous());
    }
}
