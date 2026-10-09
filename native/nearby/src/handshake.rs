use crate::{Result, require};
use ring::{
    aead, agreement,
    rand::{SecureRandom, SystemRandom},
};
use sha2::{Digest, Sha256};

pub struct Ephemeral(pub agreement::EphemeralPrivateKey);
impl Ephemeral {
    pub fn new() -> Result<Self> {
        Ok(Self(
            agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
                .map_err(|_| "Ephemeral key generation failed")?,
        ))
    }
    pub fn public(&self) -> Result<String> {
        Ok(crate::identity::encode(
            self.0
                .compute_public_key()
                .map_err(|_| "Ephemeral public key failed")?
                .as_ref(),
        ))
    }
    pub fn agree(self, remote: &str, scope: &[u8]) -> Result<aead::LessSafeKey> {
        let remote = agreement::UnparsedPublicKey::new(
            &agreement::X25519,
            crate::identity::decode(remote, 32)?,
        );
        let key = agreement::agree_ephemeral(self.0, &remote, |secret| {
            let mut hash = Sha256::new();
            hash.update(b"arkos-nearby/credential/v1\0");
            hash.update(secret);
            hash.update(scope);
            hash.finalize()
        })
        .map_err(|_| "Peer key agreement failed")?;
        Ok(aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &key)
                .map_err(|_| "Credential encryption key failed")?,
        ))
    }
}
pub fn seal(key: &aead::LessSafeKey, scope: &[u8], credential: &str) -> Result<(String, String)> {
    require(
        (8..=63).contains(&credential.len()),
        "Invalid local wireless credential",
    )?;
    let mut nonce = [0; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| "Credential nonce failed")?;
    let mut bytes = credential.as_bytes().to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(scope),
        &mut bytes,
    )
    .map_err(|_| "Credential encryption failed")?;
    Ok((
        crate::identity::encode(&nonce),
        crate::identity::encode(&bytes),
    ))
}
pub fn open(key: &aead::LessSafeKey, scope: &[u8], nonce: &str, encrypted: &str) -> Result<String> {
    require(
        encrypted.len() >= 48 && encrypted.len() <= 158 && encrypted.len() % 2 == 0,
        "Invalid encrypted credential",
    )?;
    let nonce: [u8; 12] = crate::identity::decode(nonce, 12)?.try_into().unwrap();
    let mut bytes = crate::identity::decode(encrypted, encrypted.len() / 2)?;
    let plain = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(scope),
            &mut bytes,
        )
        .map_err(|_| "Credential authentication failed")?;
    let value = String::from_utf8(plain.to_vec())?;
    require(
        (8..=63).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_alphanumeric()),
        "Invalid provisioned peer credential",
    )?;
    Ok(value)
}
