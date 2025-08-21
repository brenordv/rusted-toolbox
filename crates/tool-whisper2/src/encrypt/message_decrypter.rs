use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};
use tracing::debug;

#[derive(Clone)]
pub struct MessageDecrypter {
    private_key: RsaPrivateKey,
}

impl MessageDecrypter {
    pub fn new(private_key: RsaPrivateKey) -> Self {
        Self { private_key }
    }

    pub fn decrypt_message(&self, msg: &str) -> Result<String> {
        debug!("Decrypting message...");
        let bytes = BASE64.decode(msg)?;
        let decrypted = self.private_key.decrypt(Pkcs1v15Encrypt, &bytes)?;
        Ok(String::from_utf8(decrypted)?)
    }
}
