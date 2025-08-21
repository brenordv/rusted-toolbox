use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand06_compat::Rand0_6CompatExt;
use rsa::pkcs8::EncodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use tracing::debug;

#[derive(Clone)]
pub struct MessageEncrypter {
    public_key: RsaPublicKey,
}

impl MessageEncrypter {
    pub fn new(public_key: RsaPublicKey) -> Result<MessageEncrypter> {
        Ok(MessageEncrypter { public_key })
    }

    pub fn get_public_key(&self) -> Result<String> {
        debug!("Getting public key...");
        let der_bytes = self.public_key.to_public_key_der()?;
        Ok(BASE64.encode(der_bytes.as_bytes()))
    }

    pub fn get_pub_key_fingerprint(&self) -> Result<String> {
        debug!("Getting public key fingerprint...");
        let pub_key_b64 = self.get_public_key()?;
        let hash = sha256::digest(pub_key_b64.as_bytes());
        Ok(hash[..12].to_string().to_uppercase())
    }

    pub fn encrypt_message(&self, msg: &String) -> Result<String> {
        debug!("Encrypting message...");
        let rng = rand::rng();
        let mut rng06 = rng.compat();
        let msg_bytes = msg.as_bytes();
        let encrypted = self
            .public_key
            .encrypt(&mut rng06, Pkcs1v15Encrypt, msg_bytes)?;
        Ok(BASE64.encode(encrypted))
    }
}
