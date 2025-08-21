use crate::encrypt::message_decrypter::MessageDecrypter;
use crate::encrypt::message_encrypter::MessageEncrypter;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand06_compat::Rand0_6CompatExt;
use rsa::pkcs8::DecodePublicKey;
use rsa::{RsaPrivateKey, RsaPublicKey};
use tracing::info;

const BITS: usize = 4096;

#[derive(Clone)]
pub struct Encryption {
    encrypter: MessageEncrypter,
    decrypter: MessageDecrypter,
}

impl Encryption {
    pub fn new_keypair() -> Result<Self> {
        info!("Generating new keypair with {} bits...", BITS);

        let rng = rand::rng();
        let mut rng06 = rng.compat();
        let private_key = RsaPrivateKey::new(&mut rng06, BITS)?;
        let public_key = RsaPublicKey::from(&private_key);

        let keypair = Self {
            encrypter: MessageEncrypter::new(public_key)?,
            decrypter: MessageDecrypter::new(private_key),
        };

        let fingerprint = keypair.get_pub_key_fingerprint()?;

        info!("Generated keypair with fingerprint: {}", fingerprint);
        Ok(keypair)
    }

    pub fn create_pub_key_from_base64(pub_key_base64: &str) -> Result<RsaPublicKey> {
        let der_bytes = BASE64.decode(pub_key_base64)?;
        Ok(RsaPublicKey::from_public_key_der(&der_bytes)?)
    }

    pub fn get_public_key(&self) -> Result<String> {
        self.encrypter.get_public_key()
    }

    pub fn get_pub_key_fingerprint(&self) -> Result<String> {
        self.encrypter.get_pub_key_fingerprint()
    }

    pub fn get_decrypter(&self) -> &MessageDecrypter {
        &self.decrypter
    }
}
