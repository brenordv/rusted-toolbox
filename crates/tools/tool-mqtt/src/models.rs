pub struct MqttConfig {
    pub command: MqttCommand,
    pub host: String,
    pub port: u16,
    pub topic: String,
    pub message: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl MqttConfig {
    pub fn is_anonymous(&self) -> bool {
        self.username.is_none() && self.password.is_none()
    }
}

pub enum MqttCommand {
    Read,
    Post,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with_credentials(username: Option<&str>, password: Option<&str>) -> MqttConfig {
        MqttConfig {
            command: MqttCommand::Read,
            host: "localhost".to_string(),
            port: 1883,
            topic: "sensors/temp".to_string(),
            message: None,
            username: username.map(str::to_string),
            password: password.map(str::to_string),
        }
    }

    #[test]
    fn is_anonymous_true_when_no_credentials() {
        assert!(args_with_credentials(None, None).is_anonymous());
    }

    #[test]
    fn is_anonymous_false_when_username_present() {
        assert!(!args_with_credentials(Some("user"), None).is_anonymous());
    }

    #[test]
    fn is_anonymous_false_when_password_present() {
        assert!(!args_with_credentials(None, Some("pass")).is_anonymous());
    }

    #[test]
    fn is_anonymous_false_when_both_present() {
        assert!(!args_with_credentials(Some("user"), Some("pass")).is_anonymous());
    }
}
