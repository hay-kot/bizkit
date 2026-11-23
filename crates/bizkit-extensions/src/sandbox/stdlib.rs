//! The host side of the script globals `crypto`, `encoding`, `URL` and
//! `URLSearchParams`. Every helper takes and returns strings: scripts have no
//! typed arrays, so bytes cross as UTF-8 text, hex or base64.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::json;
use sha1::Sha1;
use sha2::{Digest as _, Sha256};
use url::Url;

/// The most data `digest` and `hmac` take in one call.
pub(super) const MAX_DIGEST_BYTES: usize = 1 << 20;

/// What a helper returns to the prelude, which throws the error.
pub(super) type HostResult = Result<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Alg {
    Sha256,
    Sha1,
}

fn alg(name: &str) -> Result<Alg, String> {
    match name {
        "sha256" => Ok(Alg::Sha256),
        "sha1" => Ok(Alg::Sha1),
        other => Err(format!("unknown algorithm {other:?}; use \"sha256\" or \"sha1\"")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bytes {
    Utf8,
    Hex,
    Base64,
}

fn bytes_encoding(name: &str, allow_utf8: bool) -> Result<Bytes, String> {
    match name {
        "hex" => Ok(Bytes::Hex),
        "base64" => Ok(Bytes::Base64),
        "utf8" if allow_utf8 => Ok(Bytes::Utf8),
        other => Err(format!("unknown encoding {other:?}")),
    }
}

fn encode_bytes(bytes: &[u8], encoding: Bytes) -> String {
    match encoding {
        Bytes::Hex => hex::encode(bytes),
        Bytes::Base64 => STANDARD.encode(bytes),
        Bytes::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn decode_bytes(text: &str, encoding: Bytes) -> Result<Vec<u8>, String> {
    match encoding {
        Bytes::Utf8 => Ok(text.as_bytes().to_vec()),
        Bytes::Hex => hex::decode(text).map_err(|err| format!("the key is not hex: {err}")),
        Bytes::Base64 => {
            STANDARD.decode(text).map_err(|err| format!("the key is not base64: {err}"))
        }
    }
}

fn check_size(data: &str) -> Result<(), String> {
    if data.len() > MAX_DIGEST_BYTES {
        return Err(format!("the data is over {} KiB", MAX_DIGEST_BYTES >> 10));
    }
    Ok(())
}

/// `crypto.digest(alg, data, {encoding})`.
pub(super) fn digest(alg_name: &str, data: &str, encoding: &str) -> HostResult {
    check_size(data)?;
    let encoding = bytes_encoding(encoding, false)?;
    let sum = match alg(alg_name)? {
        Alg::Sha256 => Sha256::digest(data.as_bytes()).to_vec(),
        Alg::Sha1 => Sha1::digest(data.as_bytes()).to_vec(),
    };
    Ok(encode_bytes(&sum, encoding))
}

/// `crypto.hmac(alg, key, data, {keyEncoding, encoding})`.
pub(super) fn hmac(
    alg_name: &str,
    key: &str,
    data: &str,
    key_encoding: &str,
    encoding: &str,
) -> HostResult {
    check_size(data)?;
    let key = decode_bytes(key, bytes_encoding(key_encoding, true)?)?;
    let encoding = bytes_encoding(encoding, false)?;
    let tag = match alg(alg_name)? {
        Alg::Sha256 => mac::<hmac::Hmac<Sha256>>(&key, data)?,
        Alg::Sha1 => mac::<hmac::Hmac<Sha1>>(&key, data)?,
    };
    Ok(encode_bytes(&tag, encoding))
}

fn mac<M: hmac::Mac + hmac::KeyInit>(key: &[u8], data: &str) -> Result<Vec<u8>, String> {
    let mut mac = <M as hmac::KeyInit>::new_from_slice(key).map_err(|err| err.to_string())?;
    mac.update(data.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

/// `crypto.timingSafeEqual(a, b)`. The time depends on the lengths only.
#[must_use]
pub(super) fn timing_safe_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// `crypto.randomUUID()`: a version 4 UUID.
#[must_use]
pub(super) fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// `encoding.base64Encode(text, {url})`. URL-safe output has no padding.
#[must_use]
pub(super) fn b64_encode(text: &str, url: bool) -> String {
    if url { URL_SAFE_NO_PAD.encode(text) } else { STANDARD.encode(text) }
}

/// `encoding.base64Decode(b64, {url})`. URL-safe input may keep its padding.
pub(super) fn b64_decode(b64: &str, url: bool) -> HostResult {
    let bytes =
        if url { URL_SAFE_NO_PAD.decode(b64.trim_end_matches('=')) } else { STANDARD.decode(b64) }
            .map_err(|err| format!("not base64: {err}"))?;
    utf8(bytes)
}

/// `encoding.hexEncode(text)`.
#[must_use]
pub(super) fn hex_encode(text: &str) -> String {
    hex::encode(text)
}

/// `encoding.hexDecode(hex)`.
pub(super) fn hex_decode(text: &str) -> HostResult {
    utf8(hex::decode(text).map_err(|err| format!("not hex: {err}"))?)
}

fn utf8(bytes: Vec<u8>) -> HostResult {
    String::from_utf8(bytes)
        .map_err(|_| "the decoded bytes are not UTF-8 text; scripts work on text only".to_owned())
}

/// `new URL(input, base)`: the parts as JSON, or why it isn't a URL.
pub(super) fn parse_url(input: &str, base: Option<&str>) -> HostResult {
    let url = match base {
        Some(base) => {
            let base = Url::parse(base).map_err(|err| format!("invalid base URL: {err}"))?;
            base.join(input)
        }
        None => Url::parse(input),
    }
    .map_err(|err| format!("invalid URL: {err}"))?;
    Ok(parts(&url))
}

/// Sets `pathname`, `search` or `hash` on `href` and gives the new parts.
pub(super) fn set_url(href: &str, part: &str, value: &str) -> HostResult {
    let mut url = Url::parse(href).map_err(|err| format!("invalid URL: {err}"))?;
    match part {
        "pathname" => url.set_path(value),
        "search" => {
            let query = value.strip_prefix('?').unwrap_or(value);
            url.set_query((!query.is_empty()).then_some(query));
        }
        "hash" => {
            let fragment = value.strip_prefix('#').unwrap_or(value);
            url.set_fragment((!fragment.is_empty()).then_some(fragment));
        }
        other => return Err(format!("URL.{other} is read-only")),
    }
    Ok(parts(&url))
}

fn parts(url: &Url) -> String {
    let prefixed = |prefix: char, part: Option<&str>| match part {
        Some(part) if !part.is_empty() => format!("{prefix}{part}"),
        _ => String::new(),
    };
    let port = url.port().map_or_default(|port| port.to_string());
    let hostname = url.host_str().unwrap_or_default();
    let host = if port.is_empty() { hostname.to_owned() } else { format!("{hostname}:{port}") };
    json!({
        "href": url.as_str(),
        "origin": url.origin().ascii_serialization(),
        "protocol": format!("{}:", url.scheme()),
        "username": url.username(),
        "password": url.password().unwrap_or_default(),
        "host": host,
        "hostname": hostname,
        "port": port,
        "pathname": url.path(),
        "search": prefixed('?', url.query()),
        "hash": prefixed('#', url.fragment()),
    })
    .to_string()
}

/// `URLSearchParams.toString()`: pairs as JSON `[[name, value], ...]` in,
/// `application/x-www-form-urlencoded` text out.
pub(super) fn form_encode(pairs: &str) -> HostResult {
    let pairs: Vec<(String, String)> =
        serde_json::from_str(pairs).map_err(|err| format!("search params: {err}"))?;
    Ok(url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish())
}

/// `new URLSearchParams(text)`: the pairs as JSON. A leading `?` is dropped.
#[must_use]
pub(super) fn form_decode(text: &str) -> String {
    let text = text.strip_prefix('?').unwrap_or(text);
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(text.as_bytes())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    json!(pairs).to_string()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn hmac_sha256_matches_rfc_4231() {
        // Test case 2.
        let tag = hmac("sha256", "Jefe", "what do ya want for nothing?", "utf8", "hex");
        assert_eq!(
            tag.as_deref(),
            Ok("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
        // Test case 1, with a hex key.
        let key = "0b".repeat(20);
        let hex_keyed = hmac("sha256", &key, "Hi There", "hex", "hex");
        assert_eq!(
            hex_keyed.as_deref(),
            Ok("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
        );
    }

    #[test]
    fn hmac_sha1_matches_rfc_2202() {
        let tag = hmac("sha1", "Jefe", "what do ya want for nothing?", "utf8", "hex");
        assert_eq!(tag.as_deref(), Ok("effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"));
    }

    #[test]
    fn hmac_takes_a_base64_key_and_gives_base64() {
        let key = STANDARD.encode("Jefe");
        let tag = hmac("sha256", &key, "what do ya want for nothing?", "base64", "base64");
        assert_eq!(tag.as_deref(), Ok("W9zBRr9gdU5qBCQmCJV1x1oAPwidJzmDnexYuWTsOEM="));
    }

    #[test]
    fn digests_match_known_values() {
        assert_eq!(
            digest("sha256", "abc", "hex").as_deref(),
            Ok("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            digest("sha1", "abc", "hex").as_deref(),
            Ok("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
        assert_eq!(
            digest("sha256", "", "base64").as_deref(),
            Ok("47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=")
        );
    }

    #[test]
    fn helpers_reject_bad_options() {
        assert!(digest("md5", "x", "hex").unwrap_err().contains("md5"));
        assert!(digest("sha256", "x", "utf8").is_err());
        assert!(hmac("sha256", "zz", "x", "hex", "hex").unwrap_err().contains("hex"));
        assert!(digest("sha256", &"x".repeat(MAX_DIGEST_BYTES + 1), "hex").is_err());
    }

    #[test]
    fn timing_safe_eq_compares_bytes() {
        assert!(timing_safe_eq("abc", "abc"));
        assert!(!timing_safe_eq("abc", "abd"));
        assert!(!timing_safe_eq("abc", "ab"));
        assert!(timing_safe_eq("", ""));
    }

    #[test]
    fn uuid_is_version_4() {
        let id = uuid::Uuid::parse_str(&uuid()).unwrap();
        assert_eq!(id.get_version_num(), 4);
    }

    #[test]
    fn base64_decode_refuses_binary() {
        let binary = STANDARD.encode([0xff_u8, 0xfe]);
        assert!(b64_decode(&binary, false).unwrap_err().contains("UTF-8"));
        assert!(b64_decode("not base64!", false).is_err());
        assert!(hex_decode("fffe").unwrap_err().contains("UTF-8"));
    }

    #[test]
    fn url_safe_base64_has_no_padding_and_reads_padding() {
        assert_eq!(b64_encode("a?", true), "YT8");
        assert_eq!(b64_encode("a?", false), "YT8=");
        assert_eq!(b64_decode("YT8=", true).as_deref(), Ok("a?"));
    }

    #[test]
    fn parse_url_gives_whatwg_parts() {
        let parts: serde_json::Value = serde_json::from_str(
            &parse_url("https://u:p@example.com:8443/a/b?x=1&y=2#top", None).unwrap(),
        )
        .unwrap();
        insta::assert_json_snapshot!(parts, @r###"
        {
          "hash": "#top",
          "host": "example.com:8443",
          "hostname": "example.com",
          "href": "https://u:p@example.com:8443/a/b?x=1&y=2#top",
          "origin": "https://example.com:8443",
          "password": "p",
          "pathname": "/a/b",
          "port": "8443",
          "protocol": "https:",
          "search": "?x=1&y=2",
          "username": "u"
        }
        "###);
    }

    #[test]
    fn parse_url_joins_a_base_and_reports_errors() {
        let joined = parse_url("../c?q", Some("https://example.com/a/b")).unwrap();
        assert!(joined.contains(r#""href":"https://example.com/c?q""#), "{joined}");
        assert!(parse_url("not a url", None).unwrap_err().starts_with("invalid URL"));
        assert!(parse_url("x", Some("nope")).unwrap_err().starts_with("invalid base"));
    }

    #[test]
    fn form_round_trips_known_text() {
        assert_eq!(
            form_decode("?a=1&b=two+words&a=%26"),
            r#"[["a","1"],["b","two words"],["a","&"]]"#
        );
        assert_eq!(form_encode(r#"[["a b","&="]]"#).as_deref(), Ok("a+b=%26%3D"));
    }

    proptest! {
        #[test]
        fn base64_round_trips(text in any::<String>(), url in any::<bool>()) {
            prop_assert_eq!(b64_decode(&b64_encode(&text, url), url), Ok(text));
        }

        #[test]
        fn hex_round_trips(text in any::<String>()) {
            prop_assert_eq!(hex_decode(&hex_encode(&text)), Ok(text));
        }

        #[test]
        fn search_params_round_trip(pairs in proptest::collection::vec((any::<String>(), any::<String>()), 0..6)) {
            let encoded = form_encode(&json!(pairs).to_string()).unwrap();
            let decoded: Vec<(String, String)> = serde_json::from_str(&form_decode(&encoded)).unwrap();
            prop_assert_eq!(decoded, pairs);
        }

        #[test]
        fn decoders_never_panic(text in any::<String>()) {
            drop(b64_decode(&text, false));
            drop(b64_decode(&text, true));
            drop(hex_decode(&text));
            drop(parse_url(&text, None));
            drop(form_decode(&text));
        }
    }
}
