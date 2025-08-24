use crate::models::MqttCommand;

pub trait StringExt {
    fn to_mqtt_command(&self) -> MqttCommand;
}

impl StringExt for String {
    fn to_mqtt_command(&self) -> MqttCommand {
        match self.trim().to_lowercase().as_str() {
            "reads" | "read" => MqttCommand::Read,
            "post" | "send" => MqttCommand::Post,
            _ => MqttCommand::Unknown,
        }
    }
}
