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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encrypt::message_encrypter::MessageEncrypter;
    use rand06_compat::Rand0_6CompatExt;
    use rsa::RsaPublicKey;
    use std::sync::LazyLock;

    static TEST_KEYPAIR: LazyLock<(RsaPrivateKey, RsaPublicKey)> = LazyLock::new(|| {
        let rng = rand::rng();
        let mut rng06 = rng.compat();
        let private_key = RsaPrivateKey::new(&mut rng06, 2048).expect("key generation");
        let public_key = RsaPublicKey::from(&private_key);
        (private_key, public_key)
    });

    fn test_keypair() -> (RsaPrivateKey, RsaPublicKey) {
        TEST_KEYPAIR.clone()
    }

    #[test]
    fn decrypt_message_roundtrips_encrypted_text() {
        let (private_key, public_key) = test_keypair();
        let decrypter = MessageDecrypter::new(private_key);
        let encrypter = MessageEncrypter::new(public_key).unwrap();

        let ciphertext = encrypter.encrypt_message("top secret").unwrap();
        let plaintext = decrypter.decrypt_message(&ciphertext).unwrap();

        assert_eq!(plaintext, "top secret");
    }

    #[test]
    fn decrypt_message_rejects_invalid_base64() {
        let (private_key, _public_key) = test_keypair();
        let decrypter = MessageDecrypter::new(private_key);

        assert!(decrypter.decrypt_message("not valid base64 !!!").is_err());
    }

    #[test]
    fn decrypt_message_rejects_valid_base64_that_is_not_ciphertext() {
        let (private_key, _public_key) = test_keypair();
        let decrypter = MessageDecrypter::new(private_key);
        let garbage = BASE64.encode(b"this is not an rsa ciphertext");

        assert!(decrypter.decrypt_message(&garbage).is_err());
    }
}
