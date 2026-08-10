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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_mqtt_command_read_aliases_map_to_read() {
        assert!(matches!(
            "read".to_string().to_mqtt_command(),
            MqttCommand::Read
        ));
        assert!(matches!(
            "reads".to_string().to_mqtt_command(),
            MqttCommand::Read
        ));
    }

    #[test]
    fn to_mqtt_command_post_aliases_map_to_post() {
        assert!(matches!(
            "post".to_string().to_mqtt_command(),
            MqttCommand::Post
        ));
        assert!(matches!(
            "send".to_string().to_mqtt_command(),
            MqttCommand::Post
        ));
    }

    #[test]
    fn to_mqtt_command_trims_and_ignores_case() {
        assert!(matches!(
            "  READ  ".to_string().to_mqtt_command(),
            MqttCommand::Read
        ));
        assert!(matches!(
            "Send".to_string().to_mqtt_command(),
            MqttCommand::Post
        ));
    }

    #[test]
    fn to_mqtt_command_unrecognized_maps_to_unknown() {
        assert!(matches!(
            "delete".to_string().to_mqtt_command(),
            MqttCommand::Unknown
        ));
        assert!(matches!(
            "".to_string().to_mqtt_command(),
            MqttCommand::Unknown
        ));
    }
}
