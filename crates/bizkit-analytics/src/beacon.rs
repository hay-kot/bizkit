//! The beacon the snippet sends: one event as `text/plain` JSON. Parsing is
//! pure and fuzzed; the ingest route drops anything it rejects.

use std::fmt;

use serde::Deserialize;
use serde_json::{Map, Value};
use url::Url;

use crate::names;

/// The largest beacon body.
pub const MAX_BODY: usize = 8 * 1024;
/// The longest page or referrer URL.
pub const MAX_URL: usize = 2048;
/// The most keys in props or traits.
pub const MAX_PROPS: usize = 20;
/// The longest prop key, in characters.
pub const MAX_PROP_KEY: usize = 64;
/// The longest string prop value, in characters.
pub const MAX_PROP_VALUE: usize = 256;
/// The longest user ID, in characters.
pub const MAX_USER_ID: usize = 128;
/// The longest anonymous ID.
pub const MAX_ANON_ID: usize = 64;
/// The longest site key.
pub const MAX_SITE_KEY: usize = 64;
/// The event name of a pageview.
pub const PAGEVIEW: &str = "pageview";
/// The event name that links a user ID to the browser.
pub const IDENTIFY: &str = "$identify";

/// What an event is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Name {
    /// A page was viewed.
    Pageview,
    /// The site called `identify`.
    Identify,
    /// A custom event from `track`.
    Custom(String),
}

impl Name {
    /// Reads an event name: `pageview`, `$identify`, or a custom name.
    pub(crate) fn parse(text: &str) -> Result<Self, Reject> {
        match text {
            PAGEVIEW => Ok(Self::Pageview),
            IDENTIFY => Ok(Self::Identify),
            other if names::is_event_name(other) => Ok(Self::Custom(other.to_owned())),
            _ => Err(Reject::Name),
        }
    }

    /// The stored name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pageview => PAGEVIEW,
            Self::Identify => IDENTIFY,
            Self::Custom(name) => name,
        }
    }
}

/// A parsed beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beacon {
    /// The site key.
    pub site: String,
    /// The event.
    pub name: Name,
    /// The page the event happened on.
    pub url: Url,
    /// The page that linked here, when it parses as http(s).
    pub referrer: Option<Url>,
    /// The window width in CSS pixels.
    pub width: Option<u32>,
    /// Event properties.
    pub props: Option<Map<String, Value>>,
    /// The browser's stored anonymous ID.
    pub anon_id: Option<String>,
    /// The user ID, for `$identify`.
    pub user_id: Option<String>,
    /// User traits, for `$identify`.
    pub traits: Option<Map<String, Value>>,
}

/// Why a beacon was dropped. Only logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// Over [`MAX_BODY`].
    TooLarge,
    /// Not a JSON object of the expected shape.
    Malformed,
    /// The site key isn't one.
    Site,
    /// The event name isn't one.
    Name,
    /// The page URL isn't an http(s) URL.
    Url,
    /// Props or traits break the limits.
    Props,
    /// The anonymous or user ID breaks the limits.
    Id,
}

impl fmt::Display for Reject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "body too large",
            Self::Malformed => "malformed body",
            Self::Site => "bad site key",
            Self::Name => "bad event name",
            Self::Url => "bad page URL",
            Self::Props => "bad props or traits",
            Self::Id => "bad anonymous or user ID",
        })
    }
}

#[derive(Deserialize)]
struct Raw {
    s: String,
    n: String,
    u: String,
    #[serde(default)]
    r: Option<String>,
    #[serde(default)]
    w: Option<Value>,
    #[serde(default)]
    p: Option<Map<String, Value>>,
    #[serde(default)]
    a: Option<String>,
    #[serde(default)]
    i: Option<String>,
    #[serde(default)]
    t: Option<Map<String, Value>>,
}

/// Parses a beacon body.
///
/// # Errors
///
/// [`Reject`] says what was wrong.
pub fn parse(body: &[u8]) -> Result<Beacon, Reject> {
    if body.len() > MAX_BODY {
        return Err(Reject::TooLarge);
    }
    let raw: Raw = serde_json::from_slice(body).map_err(|_| Reject::Malformed)?;
    if raw.s.is_empty()
        || raw.s.len() > MAX_SITE_KEY
        || !raw.s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Reject::Site);
    }
    let name = Name::parse(&raw.n)?;
    let url = page_url(&raw.u).ok_or(Reject::Url)?;
    let referrer = raw.r.as_deref().and_then(page_url);
    let width = raw.w.as_ref().and_then(Value::as_u64).and_then(|w| u32::try_from(w).ok());
    let props = raw.p.map(check_props).transpose()?;
    let traits = raw.t.map(check_props).transpose()?;
    let anon_id = raw.a.map(|id| check_anon_id(&id).then_some(id).ok_or(Reject::Id)).transpose()?;
    let user_id = raw.i.map(|id| check_user_id(&id).then_some(id).ok_or(Reject::Id)).transpose()?;
    if name == Name::Identify && user_id.is_none() {
        return Err(Reject::Id);
    }
    Ok(Beacon { site: raw.s, name, url, referrer, width, props, anon_id, user_id, traits })
}

/// Reads an http(s) URL of at most [`MAX_URL`] bytes.
pub(crate) fn page_url(text: &str) -> Option<Url> {
    if text.len() > MAX_URL {
        return None;
    }
    Url::parse(text)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

/// Checks props or traits against the limits and drops null values.
///
/// # Errors
///
/// [`Reject::Props`] when a key or value breaks them.
pub(crate) fn check_props(map: Map<String, Value>) -> Result<Map<String, Value>, Reject> {
    if map.len() > MAX_PROPS {
        return Err(Reject::Props);
    }
    let mut out = Map::new();
    for (key, value) in map {
        let key_chars = key.chars().count();
        if key_chars == 0 || key_chars > MAX_PROP_KEY || key.chars().any(char::is_control) {
            return Err(Reject::Props);
        }
        match &value {
            Value::Null => continue,
            Value::Bool(_) | Value::Number(_) => {}
            Value::String(text) if text.chars().count() <= MAX_PROP_VALUE => {}
            Value::String(_) | Value::Array(_) | Value::Object(_) => return Err(Reject::Props),
        }
        out.insert(key, value);
    }
    Ok(out)
}

/// Whether `id` is an anonymous ID: 1-64 of `[A-Za-z0-9_-]`.
pub(crate) fn check_anon_id(id: &str) -> bool {
    (1..=MAX_ANON_ID).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Whether `id` is a user ID: 1-128 characters, no control characters, not
/// only whitespace.
pub(crate) fn check_user_id(id: &str) -> bool {
    let chars = id.chars().count();
    (1..=MAX_USER_ID).contains(&chars) && !id.chars().any(char::is_control) && !id.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    #[test]
    fn parses_a_pageview() {
        let beacon = parse(&body(&json!({
            "s": "abc123", "n": "pageview", "u": "https://example.com/pricing?utm_source=hn",
            "r": "https://news.ycombinator.com/", "w": 1440,
        })))
        .unwrap();
        assert_eq!(beacon.name, Name::Pageview);
        assert_eq!(beacon.url.path(), "/pricing");
        assert_eq!(beacon.referrer.unwrap().host_str(), Some("news.ycombinator.com"));
        assert_eq!(beacon.width, Some(1440));
        assert_eq!(beacon.props, None);
    }

    #[test]
    fn parses_custom_events_and_identify() {
        let beacon = parse(&body(&json!({
            "s": "abc", "n": "signup", "u": "https://x.test/", "p": {"plan": "pro", "seats": 3, "x": null},
            "a": "3f1c-aa",
        })))
        .unwrap();
        assert_eq!(beacon.name, Name::Custom("signup".to_owned()));
        assert_eq!(
            beacon.props,
            Some(json!({"plan": "pro", "seats": 3}).as_object().cloned().unwrap())
        );
        assert_eq!(beacon.anon_id.as_deref(), Some("3f1c-aa"));

        let identify = parse(&body(&json!({
            "s": "abc", "n": "$identify", "u": "https://x.test/", "i": "user_1", "t": {"email": "a@x.test"},
        })))
        .unwrap();
        assert_eq!(identify.name, Name::Identify);
        assert_eq!(identify.user_id.as_deref(), Some("user_1"));
    }

    #[test]
    fn rejects() {
        let cases = [
            (json!({"s": "", "n": "pageview", "u": "https://x.test/"}), Reject::Site),
            (json!({"s": "a b", "n": "pageview", "u": "https://x.test/"}), Reject::Site),
            (json!({"s": "a", "n": "bad name", "u": "https://x.test/"}), Reject::Name),
            (json!({"s": "a", "n": "pageview", "u": "javascript:alert(1)"}), Reject::Url),
            (json!({"s": "a", "n": "pageview", "u": "file:///etc/passwd"}), Reject::Url),
            (json!({"s": "a", "n": "x", "u": "https://x.test/", "p": {"k": [1]}}), Reject::Props),
            (
                json!({"s": "a", "n": "x", "u": "https://x.test/", "p": {"k": "v".repeat(257)}}),
                Reject::Props,
            ),
            (json!({"s": "a", "n": "x", "u": "https://x.test/", "a": "has space"}), Reject::Id),
            (json!({"s": "a", "n": "$identify", "u": "https://x.test/"}), Reject::Id),
            (json!({"n": "pageview", "u": "https://x.test/"}), Reject::Malformed),
            (json!([1, 2]), Reject::Malformed),
        ];
        for (value, want) in cases {
            assert_eq!(parse(&body(&value)), Err(want), "{value}");
        }
        let many: Map<String, Value> = (0..21).map(|i| (format!("k{i}"), json!(i))).collect();
        assert_eq!(check_props(many), Err(Reject::Props));
        assert_eq!(parse(&vec![b' '; MAX_BODY + 1]), Err(Reject::TooLarge));
    }

    #[test]
    fn a_bad_referrer_or_width_is_dropped_not_rejected() {
        let beacon = parse(&body(&json!({
            "s": "a", "n": "pageview", "u": "https://x.test/", "r": "android-app://x", "w": -5,
        })))
        .unwrap();
        assert_eq!(beacon.referrer, None);
        assert_eq!(beacon.width, None);
    }

    /// The fuzz harness: under `cargo test` it runs as a property test, and
    /// `mise run fuzz` runs it under libFuzzer.
    #[test]
    fn fuzz_parse() {
        bolero::check!().for_each(|body: &[u8]| {
            if let Ok(beacon) = parse(body) {
                assert!(body.len() <= MAX_BODY, "size");
                assert!(!beacon.site.is_empty() && beacon.site.len() <= MAX_SITE_KEY, "site");
                assert!(matches!(beacon.url.scheme(), "http" | "https"), "scheme");
                for map in [&beacon.props, &beacon.traits].into_iter().flatten() {
                    assert!(map.len() <= MAX_PROPS, "props");
                    assert!(map.values().all(|v| !v.is_null() && !v.is_array() && !v.is_object()));
                }
                if beacon.name == Name::Identify {
                    assert!(beacon.user_id.is_some(), "identify needs a user");
                }
            }
        });
    }
}
