use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand06_compat::Rand0_6CompatExt;
use rsa::pkcs8::EncodePublicKey;
use rsa::traits::PublicKeyParts;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use tracing::debug;

/// PKCS#1 v1.5 padding overhead in bytes: RFC 8017 section 7.2.1 caps the plaintext at k - 11,
/// where k is the modulus size in octets.
const PKCS1V15_OVERHEAD: usize = 11;

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

    /// Largest plaintext, in bytes, that fits one PKCS#1 v1.5 block for this key.
    pub fn max_plaintext_len(&self) -> usize {
        self.public_key.size().saturating_sub(PKCS1V15_OVERHEAD)
    }

    pub fn encrypt_message(&self, msg: &str) -> Result<String> {
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
    use std::sync::LazyLock;

    static TEST_PUBLIC_KEY: LazyLock<RsaPublicKey> = LazyLock::new(|| {
        let rng = rand::rng();
        let mut rng06 = rng.compat();
        let private_key = RsaPrivateKey::new(&mut rng06, 2048).expect("key generation");
        RsaPublicKey::from(&private_key)
    });

    fn test_encrypter() -> MessageEncrypter {
        MessageEncrypter::new(TEST_PUBLIC_KEY.clone()).expect("encrypter from test key")
    }

    #[test]
    fn get_public_key_returns_decodable_base64() {
        let encrypter = test_encrypter();

        let encoded = encrypter.get_public_key().unwrap();

        assert!(BASE64.decode(&encoded).is_ok());
    }

    #[test]
    fn get_pub_key_fingerprint_is_stable_and_hex_formatted() {
        let encrypter = test_encrypter();

        let first = encrypter.get_pub_key_fingerprint().unwrap();
        let second = encrypter.get_pub_key_fingerprint().unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 12);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!first.chars().any(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn encrypt_message_returns_non_empty_base64() {
        let encrypter = test_encrypter();

        let ciphertext = encrypter.encrypt_message("hello").unwrap();

        assert!(!ciphertext.is_empty());
        assert!(BASE64.decode(&ciphertext).is_ok());
    }

    #[test]
    fn max_plaintext_len_for_2048_bit_key_is_245() {
        assert_eq!(test_encrypter().max_plaintext_len(), 245);
    }

    #[test]
    fn encrypt_message_accepts_plaintext_at_the_pkcs1v15_limit() {
        let encrypter = test_encrypter();
        let message = "a".repeat(245);

        assert!(encrypter.encrypt_message(&message).is_ok());
    }

    #[test]
    fn encrypt_message_rejects_plaintext_over_the_pkcs1v15_limit() {
        let encrypter = test_encrypter();
        let message = "a".repeat(246);

        assert!(encrypter.encrypt_message(&message).is_err());
    }
}
