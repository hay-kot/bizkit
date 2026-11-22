//! Sign-in by magic link, sessions, API keys and the email policies that
//! decide who may have an account.

pub(crate) mod api_keys;
pub(crate) mod magic_links;
pub(crate) mod middleware;
pub(crate) mod ops;
pub mod policies;
pub(crate) mod routes;
pub(crate) mod sessions;
pub(crate) mod users;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngExt as _;
use sha2::{Digest as _, Sha256};

/// The session cookie name.
pub const SESSION_COOKIE: &str = "bizkit_session";

/// The prefix every API key token starts with.
pub const API_KEY_PREFIX: &str = "bk_";

/// 32 random bytes in unpadded base64url.
#[must_use]
pub fn random_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The SHA-256 of a token in hex. Tokens are 256 random bits, so a fast hash
/// is enough; the database never holds a usable token.
#[must_use]
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut out, byte| {
        use std::fmt::Write as _;
        // Writing to a String can't fail.
        let _ignored = write!(out, "{byte:02x}");
        out
    })
}

/// Trims and lowercases an email, the form the database stores.
#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// A cheap shape check: one `@`, text on both sides, a dot in the domain, no
/// whitespace, at most 254 characters. Delivery is the real check.
#[must_use]
pub fn is_plausible_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    email.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && !email.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_43_url_safe_chars() {
        let token = random_token();
        assert_eq!(token.len(), 43, "32 bytes in unpadded base64");
        assert!(
            token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "url safe"
        );
    }

    #[test]
    fn hashes_are_hex_sha256() {
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "sha256"
        );
    }

    #[test]
    fn email_checks() {
        assert_eq!(normalize_email("  A@Example.COM "), "a@example.com", "normalized");
        assert!(is_plausible_email("a@example.com"), "plain");
        for bad in ["a", "@example.com", "a@example", "a@@example.com", "a b@example.com", "a@.com"]
        {
            assert!(!is_plausible_email(bad), "{bad} should be rejected");
        }
    }
}
