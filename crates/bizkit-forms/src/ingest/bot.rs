//! Bot checks: the honeypot field, and an ALTCHA v1 compatible proof-of-work
//! challenge that also carries a signed issue time for the timing check.
//!
//! The wire format matches <https://github.com/altcha-org/altcha>, so the
//! MIT ALTCHA widget works with `challengeurl` set to the challenge route.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bizkit_core::time::UnixMillis;
use hmac::{KeyInit as _, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use utoipa::ToSchema;

use super::parse::RawFields;
use crate::api::FormId;

type HmacSha256 = hmac::Hmac<Sha256>;

/// The honeypot field. Humans never see it, so any value means a bot.
pub const HONEYPOT_FIELD: &str = "_gotcha";
/// The proof-of-work field, the ALTCHA widget's default name.
pub const ALTCHA_FIELD: &str = "altcha";
/// How long a challenge stays valid after it is issued.
pub const CHALLENGE_TTL: Duration = Duration::from_hours(1);
const ALGORITHM: &str = "SHA-256";

/// What makes a submission spam. Stored as the submission's `spam_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpamReason {
    /// The honeypot field has a value.
    Honeypot,
    /// The form asks for a challenge and none came.
    MissingProof,
    /// The payload doesn't decode, is for another form, or doesn't verify.
    BadProof,
    /// Submitted sooner than the form's minimum fill time.
    TooFast,
    /// The challenge is past its expiry.
    Expired,
    /// The challenge was already used.
    Replayed,
}

impl SpamReason {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Honeypot => "honeypot",
            Self::MissingProof => "missing proof",
            Self::BadProof => "bad proof",
            Self::TooFast => "too fast",
            Self::Expired => "expired",
            Self::Replayed => "replayed",
        }
    }
}

/// The per-form inputs to issuing and verifying challenges.
#[derive(Debug, Clone, Copy)]
pub struct ChallengeSettings<'a> {
    /// The form.
    pub form_id: FormId,
    /// The form's HMAC key.
    pub key: &'a str,
    /// The largest number the solver may need to try.
    pub max_number: u32,
    /// The least time between issue and submit.
    pub min_fill_ms: u32,
}

/// An ALTCHA v1 challenge.
#[expect(clippy::struct_field_names, reason = "the field names are ALTCHA's wire format")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Challenge {
    /// Always `SHA-256`.
    pub algorithm: String,
    /// The hex SHA-256 of `salt` followed by the secret number.
    pub challenge: String,
    /// The secret number is at most this.
    pub maxnumber: u32,
    /// Random hex, then `?form=<id>&issued=<unix ms>&expires=<unix s>`.
    pub salt: String,
    /// The hex HMAC-SHA256 of `challenge` under the form's key.
    pub signature: String,
}

/// A challenge that verified. The caller records `challenge` so it can't be
/// used twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedProof {
    /// The challenge hash.
    pub challenge: String,
    /// When the challenge stops being valid.
    pub expires_at: UnixMillis,
}

/// The payload the widget posts in [`ALTCHA_FIELD`], base64-encoded JSON.
#[derive(Debug, Deserialize)]
struct Payload {
    algorithm: String,
    challenge: String,
    number: u64,
    salt: String,
    signature: String,
}

/// Spam when the honeypot field has any non-blank value.
#[must_use]
pub fn honeypot(fields: &RawFields) -> Option<SpamReason> {
    fields
        .0
        .get(HONEYPOT_FIELD)
        .is_some_and(|values| values.iter().any(|v| !v.trim().is_empty()))
        .then_some(SpamReason::Honeypot)
}

/// Issues a challenge. `random` supplies the salt bytes and the secret
/// number, so tests can fix them; the number is taken modulo `max_number + 1`.
#[must_use]
pub fn issue(
    settings: ChallengeSettings<'_>,
    now: UnixMillis,
    random: ([u8; 16], u32),
) -> Challenge {
    let (salt_bytes, number) = random;
    let number = number % settings.max_number.saturating_add(1);
    let expires_secs = now.plus(CHALLENGE_TTL).0 / 1000;
    let salt = format!(
        "{}?form={}&issued={}&expires={expires_secs}",
        hex::encode(salt_bytes),
        settings.form_id,
        now.0
    );
    let challenge = solution_hash(&salt, u64::from(number));
    let signature = sign(settings.key, &challenge);
    Challenge {
        algorithm: ALGORITHM.to_owned(),
        challenge,
        maxnumber: settings.max_number,
        salt,
        signature,
    }
}

/// Verifies the posted `altcha` payload against the form and the clock.
///
/// # Errors
///
/// The spam reason when the payload is missing, doesn't verify, came too
/// fast or expired.
pub fn verify(
    settings: ChallengeSettings<'_>,
    altcha: Option<&str>,
    now: UnixMillis,
) -> Result<VerifiedProof, SpamReason> {
    let altcha = altcha.map(str::trim).filter(|v| !v.is_empty()).ok_or(SpamReason::MissingProof)?;
    let bytes = STANDARD.decode(altcha).map_err(|_| SpamReason::BadProof)?;
    let payload: Payload = serde_json::from_slice(&bytes).map_err(|_| SpamReason::BadProof)?;
    if payload.algorithm != ALGORITHM || payload.number > u64::from(settings.max_number) {
        return Err(SpamReason::BadProof);
    }
    let params = SaltParams::parse(&payload.salt).ok_or(SpamReason::BadProof)?;
    if params.form_id != settings.form_id {
        return Err(SpamReason::BadProof);
    }
    let signature = hex::decode(&payload.signature).map_err(|_| SpamReason::BadProof)?;
    let mut mac =
        HmacSha256::new_from_slice(settings.key.as_bytes()).map_err(|_| SpamReason::BadProof)?;
    mac.update(payload.challenge.as_bytes());
    mac.verify_slice(&signature).map_err(|_| SpamReason::BadProof)?;
    if solution_hash(&payload.salt, payload.number) != payload.challenge {
        return Err(SpamReason::BadProof);
    }
    if now.0 < params.issued.0.saturating_add(i64::from(settings.min_fill_ms)) {
        return Err(SpamReason::TooFast);
    }
    let expires_at = UnixMillis(params.expires_secs.saturating_mul(1000));
    if now > expires_at {
        return Err(SpamReason::Expired);
    }
    Ok(VerifiedProof { challenge: payload.challenge, expires_at })
}

#[derive(Debug)]
struct SaltParams {
    form_id: FormId,
    issued: UnixMillis,
    expires_secs: i64,
}

impl SaltParams {
    fn parse(salt: &str) -> Option<Self> {
        let (_, query) = salt.split_once('?')?;
        let (mut form_id, mut issued, mut expires_secs) = (None, None, None);
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            match &*key {
                "form" => form_id = value.parse().ok(),
                "issued" => issued = value.parse().ok().map(UnixMillis),
                "expires" => expires_secs = value.parse().ok(),
                _ => {}
            }
        }
        Some(Self { form_id: form_id?, issued: issued?, expires_secs: expires_secs? })
    }
}

fn solution_hash(salt: &str, number: u64) -> String {
    hex::encode(Sha256::digest(format!("{salt}{number}").as_bytes()))
}

fn sign(key: &str, challenge: &str) -> String {
    // HMAC accepts keys of any length, so this can't fail.
    let mac = HmacSha256::new_from_slice(key.as_bytes()).map(|mut mac| {
        mac.update(challenge.as_bytes());
        mac.finalize().into_bytes()
    });
    mac.map_or_else(|_| String::new(), hex::encode)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "test-key";
    const NOW: UnixMillis = UnixMillis(1_800_000_000_000);

    fn settings() -> ChallengeSettings<'static> {
        ChallengeSettings {
            form_id: "0190a5d0-0000-7000-8000-000000000001".parse().unwrap(),
            key: KEY,
            max_number: 1000,
            min_fill_ms: 3000,
        }
    }

    /// Solves `challenge` the way the widget does and encodes the payload.
    fn solve(challenge: &Challenge) -> String {
        let number = (0..=u64::from(challenge.maxnumber))
            .find(|n| solution_hash(&challenge.salt, *n) == challenge.challenge)
            .unwrap();
        let payload = serde_json::json!({
            "algorithm": challenge.algorithm,
            "challenge": challenge.challenge,
            "number": number,
            "salt": challenge.salt,
            "signature": challenge.signature,
            "took": 12,
        });
        STANDARD.encode(payload.to_string())
    }

    fn issued() -> Challenge {
        issue(settings(), NOW, ([7; 16], 4242))
    }

    #[test]
    fn a_solved_challenge_verifies_after_the_fill_time() {
        let challenge = issued();
        assert_eq!(challenge.maxnumber, 1000, "max number");
        let proof = verify(settings(), Some(&solve(&challenge)), NOW.plus(Duration::from_secs(5)));
        assert_eq!(
            proof,
            Ok(VerifiedProof {
                challenge: challenge.challenge,
                expires_at: UnixMillis((NOW.0 / 1000 + 3600) * 1000),
            }),
            "verified"
        );
    }

    #[test]
    fn rejections() {
        let challenge = issued();
        let good = solve(&challenge);
        let later = NOW.plus(Duration::from_secs(5));
        assert_eq!(verify(settings(), None, later), Err(SpamReason::MissingProof), "missing");
        assert_eq!(verify(settings(), Some(" "), later), Err(SpamReason::MissingProof), "blank");
        assert_eq!(verify(settings(), Some("%%%"), later), Err(SpamReason::BadProof), "not base64");
        assert_eq!(verify(settings(), Some(&good), NOW), Err(SpamReason::TooFast), "too fast");
        assert_eq!(
            verify(settings(), Some(&good), NOW.plus(Duration::from_hours(2))),
            Err(SpamReason::Expired),
            "expired"
        );
        let other_key = ChallengeSettings { key: "other", ..settings() };
        assert_eq!(verify(other_key, Some(&good), later), Err(SpamReason::BadProof), "other key");
        let other_form = ChallengeSettings {
            form_id: "0190a5d0-0000-7000-8000-000000000002".parse().unwrap(),
            ..settings()
        };
        assert_eq!(verify(other_form, Some(&good), later), Err(SpamReason::BadProof), "other form");
    }

    #[test]
    fn a_wrong_number_fails() {
        let challenge = issued();
        let payload = serde_json::json!({
            "algorithm": challenge.algorithm,
            "challenge": challenge.challenge,
            "number": 1001,
            "salt": challenge.salt,
            "signature": challenge.signature,
        });
        let later = NOW.plus(Duration::from_secs(5));
        assert_eq!(
            verify(settings(), Some(&STANDARD.encode(payload.to_string())), later),
            Err(SpamReason::BadProof),
            "number above max"
        );
    }

    #[test]
    fn honeypot_values() {
        let mut fields = RawFields::default();
        assert_eq!(honeypot(&fields), None, "absent");
        fields.0.insert(HONEYPOT_FIELD.to_owned(), vec![String::new()]);
        assert_eq!(honeypot(&fields), None, "empty");
        fields.0.insert(HONEYPOT_FIELD.to_owned(), vec!["http://spam".to_owned()]);
        assert_eq!(honeypot(&fields), Some(SpamReason::Honeypot), "filled");
    }

    /// Arbitrary payloads never panic and never verify.
    #[test]
    fn fuzz_verify() {
        bolero::check!().with_type::<(bool, Vec<u8>)>().for_each(|(encode, bytes)| {
            let altcha = if *encode {
                STANDARD.encode(bytes)
            } else {
                String::from_utf8_lossy(bytes).into_owned()
            };
            let result = verify(settings(), Some(&altcha), NOW.plus(Duration::from_secs(5)));
            assert!(result.is_err(), "a random payload verified");
        });
    }
}
