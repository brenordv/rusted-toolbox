pub struct QrCodeConfig {
    pub payload: QrCodePayload,
    pub no_header: bool,
    pub dont_print: bool,
    pub output_format: Option<String>,
    pub output_file: Option<String>,
}

pub struct QrCodePayload {
    pub text_payload: Option<String>,
    pub wifi_ssid: Option<String>,
    pub wifi_password: Option<String>,
    pub wifi_auth: Option<String>,
}

pub enum HowMode {
    TextPayload(String),
    WifiPayload(String, String, String),
}

impl QrCodePayload {
    pub fn new(
        text_payload: Option<String>,
        wifi_ssid: Option<String>,
        wifi_password: Option<String>,
        wifi_auth: Option<String>,
    ) -> Self {
        Self {
            text_payload,
            wifi_ssid,
            wifi_password,
            wifi_auth,
        }
    }
}

impl QrCodeConfig {
    pub fn new(
        payload: QrCodePayload,
        no_header: bool,
        dont_print: bool,
        output_format: Option<String>,
        output_file: Option<String>,
    ) -> Self {
        Self {
            payload,
            no_header,
            dont_print,
            output_format,
            output_file,
        }
    }

    fn is_wifi_payload_set(&self) -> bool {
        self.payload.wifi_ssid.is_some()
            && self.payload.wifi_password.is_some()
            && self.payload.wifi_auth.is_some()
            && self.payload.text_payload.is_none()
    }

    pub fn get_payload(&self) -> HowMode {
        if self.is_wifi_payload_set() {
            HowMode::WifiPayload(
                self.payload.wifi_ssid.clone().unwrap(),
                self.payload.wifi_password.clone().unwrap(),
                self.payload.wifi_auth.clone().unwrap(),
            )
        } else {
            HowMode::TextPayload(self.payload.text_payload.clone().unwrap_or_default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(payload: QrCodePayload) -> QrCodeConfig {
        QrCodeConfig::new(payload, false, true, None, None)
    }

    #[test]
    fn get_payload_returns_text_when_text_is_set() {
        let cfg = config(QrCodePayload::new(
            Some("hello".to_string()),
            None,
            None,
            None,
        ));

        assert!(matches!(
            cfg.get_payload(),
            HowMode::TextPayload(text) if text == "hello"
        ));
    }

    #[test]
    fn get_payload_returns_wifi_when_all_wifi_fields_set() {
        let cfg = config(QrCodePayload::new(
            None,
            Some("net".to_string()),
            Some("secret".to_string()),
            Some("WPA".to_string()),
        ));

        assert!(matches!(
            cfg.get_payload(),
            HowMode::WifiPayload(ssid, password, auth)
                if ssid == "net" && password == "secret" && auth == "WPA"
        ));
    }

    #[test]
    fn get_payload_prefers_text_over_wifi() {
        let cfg = config(QrCodePayload::new(
            Some("text-wins".to_string()),
            Some("net".to_string()),
            Some("secret".to_string()),
            Some("WPA".to_string()),
        ));

        assert!(matches!(
            cfg.get_payload(),
            HowMode::TextPayload(text) if text == "text-wins"
        ));
    }

    #[test]
    fn get_payload_defaults_to_empty_text_when_nothing_set() {
        let cfg = config(QrCodePayload::new(None, None, None, None));

        assert!(matches!(
            cfg.get_payload(),
            HowMode::TextPayload(text) if text.is_empty()
        ));
    }

    #[test]
    fn get_payload_falls_back_to_text_when_wifi_incomplete() {
        let cfg = config(QrCodePayload::new(
            None,
            Some("net".to_string()),
            Some("secret".to_string()),
            None,
        ));

        assert!(matches!(cfg.get_payload(), HowMode::TextPayload(_)));
    }
}
