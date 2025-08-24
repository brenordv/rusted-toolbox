pub struct MqttArgs {
    pub command: MqttCommand,
    pub host: String,
    pub port: u16,
    pub topic: String,
    pub message: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl MqttArgs {
    pub fn is_anonymous(&self) -> bool {
        self.username.is_none() && self.password.is_none()
    }
}

pub enum MqttCommand {
    Unknown,
    Read,
    Post,
}
