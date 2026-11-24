//! Secrets at rest: values sealed with the instance key
//! (`secrets_key_env`), an XChaCha20-Poly1305 key.
//!
//! A sealed value is `0x01 || nonce (24 bytes) || ciphertext and tag`. The
//! caller passes associated data that names where the value lives (table and
//! row), so a value copied to another row fails to open.

use std::fmt;

use anyhow::Context as _;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead as _, Payload};
use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce};
use rand::RngExt as _;

const VERSION: u8 = 1;
const NONCE_LEN: usize = 24;

/// The instance key: 32 bytes.
#[derive(Clone)]
pub struct SecretKey([u8; 32]);

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(..)")
    }
}

impl SecretKey {
    /// Parses 32 bytes in standard base64, as `openssl rand -base64 32`
    /// prints them.
    ///
    /// # Errors
    ///
    /// When the text isn't base64 or doesn't decode to 32 bytes.
    pub fn from_base64(text: &str) -> anyhow::Result<Self> {
        let bytes = STANDARD.decode(text.trim()).context("the secrets key is not base64")?;
        let key: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
            anyhow::anyhow!("the secrets key is {} bytes; it must be 32", bytes.len())
        })?;
        Ok(Self(key))
    }

    /// A fresh random key.
    #[must_use]
    pub fn generate() -> Self {
        Self(rand::rng().random())
    }

    /// The key in standard base64, the form [`Self::from_base64`] reads.
    #[must_use]
    pub fn to_base64(&self) -> String {
        STANDARD.encode(self.0)
    }
}

/// Why a sealed value didn't open.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretsError {
    /// Too short, or an unknown format version.
    #[error("the sealed value is malformed")]
    Malformed,
    /// A different key sealed it, it belongs to another row, or it changed.
    #[error("the sealed value doesn't open with this key")]
    Rejected,
}

/// Seals and opens values with the instance key.
#[derive(Clone)]
pub struct Secrets {
    cipher: XChaCha20Poly1305,
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secrets(..)")
    }
}

impl Secrets {
    /// Seals and opens with `key`.
    #[must_use]
    pub fn new(key: &SecretKey) -> Self {
        Self { cipher: XChaCha20Poly1305::new(&key.0.into()) }
    }

    /// Seals `plaintext`, bound to `aad`.
    ///
    /// # Errors
    ///
    /// Only when the plaintext is too large for the cipher (about 256 GiB).
    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> anyhow::Result<Vec<u8>> {
        let nonce: [u8; NONCE_LEN] = rand::rng().random();
        let sealed = self
            .cipher
            .encrypt(&XNonce::from(nonce), Payload { msg: plaintext, aad })
            .map_err(|_| anyhow::anyhow!("sealing a secret failed"))?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + sealed.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// Opens a value [`Self::seal`] made with the same `aad`.
    ///
    /// # Errors
    ///
    /// [`SecretsError::Malformed`] for a value too short or of another
    /// version, [`SecretsError::Rejected`] when the key, the `aad` or the
    /// bytes don't match.
    pub fn open(&self, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, SecretsError> {
        let (&version, rest) = sealed.split_first().ok_or(SecretsError::Malformed)?;
        if version != VERSION {
            return Err(SecretsError::Malformed);
        }
        let (nonce, ciphertext) =
            rest.split_at_checked(NONCE_LEN).ok_or(SecretsError::Malformed)?;
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| SecretsError::Malformed)?;
        self.cipher
            .decrypt(&XNonce::from(nonce), Payload { msg: ciphertext, aad })
            .map_err(|_| SecretsError::Rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Secrets {
        Secrets::new(&SecretKey::generate())
    }

    #[test]
    fn round_trips() {
        let secrets = fresh();
        let sealed = secrets.seal(b"hunter2", b"row:1").unwrap();
        assert_eq!(secrets.open(&sealed, b"row:1").unwrap(), b"hunter2", "opens");
        assert!(!sealed.windows(7).any(|w| w == b"hunter2"), "no plaintext in the sealed bytes");
        let again = secrets.seal(b"hunter2", b"row:1").unwrap();
        assert_ne!(sealed, again, "a fresh nonce each time");
    }

    #[test]
    fn refuses_another_row_another_key_and_tampering() {
        let secrets = fresh();
        let sealed = secrets.seal(b"hunter2", b"row:1").unwrap();
        assert_eq!(secrets.open(&sealed, b"row:2"), Err(SecretsError::Rejected), "other row");
        assert_eq!(fresh().open(&sealed, b"row:1"), Err(SecretsError::Rejected), "other key");
        let mut flipped = sealed.clone();
        if let Some(last) = flipped.last_mut() {
            *last ^= 1;
        }
        assert_eq!(secrets.open(&flipped, b"row:1"), Err(SecretsError::Rejected), "tampered");
        let mut versioned = sealed;
        versioned[0] = 2;
        assert_eq!(secrets.open(&versioned, b"row:1"), Err(SecretsError::Malformed), "version");
        assert_eq!(secrets.open(&[1, 2, 3], b"row:1"), Err(SecretsError::Malformed), "short");
        assert_eq!(secrets.open(&[], b"row:1"), Err(SecretsError::Malformed), "empty");
    }

    #[test]
    fn parses_base64_keys_of_32_bytes() {
        let key = SecretKey::generate();
        let parsed = SecretKey::from_base64(&key.to_base64()).unwrap();
        assert_eq!(parsed.0, key.0, "round trip");
        let short = SecretKey::from_base64(&STANDARD.encode([0u8; 16])).unwrap_err();
        assert!(short.to_string().contains("16 bytes"), "{short}");
        assert!(SecretKey::from_base64("not base64!").is_err(), "junk");
        assert_eq!(format!("{key:?}"), "SecretKey(..)", "debug hides the key");
    }
}
