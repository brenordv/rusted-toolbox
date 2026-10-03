use crate::errors::{MSG_PASSPHRASE_REFUSED, map_copy_error, map_decrypt_error, map_encrypt_error};
use age::armor::{ArmoredReader, ArmoredWriter, Format};
use age::x25519::{Identity, Recipient};
use anyhow::bail;
use std::io;

pub fn encrypt(
    recipients: &[Recipient],
    input: &mut impl io::Read,
    output: &mut impl io::Write,
    armor: bool,
) -> anyhow::Result<u64> {
    let format = if armor {
        Format::AsciiArmor
    } else {
        Format::Binary
    };

    let encryptor = age::Encryptor::with_recipients(
        recipients
            .iter()
            .map(|recipient| recipient as &dyn age::Recipient),
    )
    .map_err(map_encrypt_error)?;

    let armored = ArmoredWriter::wrap_output(&mut *output, format)?;

    let mut writer = encryptor.wrap_output(armored)?;

    let bytes = io::copy(input, &mut writer)?;

    writer.finish().and_then(|armored| armored.finish())?;

    Ok(bytes)
}

pub fn decrypt(
    identities: &[Identity],
    input: &mut impl io::Read,
    output: &mut impl io::Write,
) -> anyhow::Result<u64> {
    let decryptor =
        age::Decryptor::new(ArmoredReader::new(&mut *input)).map_err(map_decrypt_error)?;

    if decryptor.is_scrypt() {
        bail!(MSG_PASSPHRASE_REFUSED);
    }

    let mut reader = decryptor
        .decrypt(identities.iter().map(|i| i as &dyn age::Identity))
        .map_err(map_decrypt_error)?;

    let bytes = io::copy(&mut reader, output).map_err(map_copy_error)?;
    Ok(bytes)
}
