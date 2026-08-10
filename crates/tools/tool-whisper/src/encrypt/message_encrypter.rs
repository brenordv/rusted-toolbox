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

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::RsaPrivateKey;

    fn test_public_key() -> RsaPublicKey {
        let rng = rand::rng();
        let mut rng06 = rng.compat();
        let private_key = RsaPrivateKey::new(&mut rng06, 2048).expect("key generation");
        RsaPublicKey::from(&private_key)
    }

    #[test]
    fn get_public_key_returns_decodable_base64() {
        let encrypter = MessageEncrypter::new(test_public_key()).unwrap();

        let encoded = encrypter.get_public_key().unwrap();

        assert!(BASE64.decode(&encoded).is_ok());
    }

    #[test]
    fn get_pub_key_fingerprint_is_stable_and_hex_formatted() {
        let encrypter = MessageEncrypter::new(test_public_key()).unwrap();

        let first = encrypter.get_pub_key_fingerprint().unwrap();
        let second = encrypter.get_pub_key_fingerprint().unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 12);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!first.chars().any(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn encrypt_message_returns_non_empty_base64() {
        let encrypter = MessageEncrypter::new(test_public_key()).unwrap();

        let ciphertext = encrypter.encrypt_message(&"hello".to_string()).unwrap();

        assert!(!ciphertext.is_empty());
        assert!(BASE64.decode(&ciphertext).is_ok());
    }
}
