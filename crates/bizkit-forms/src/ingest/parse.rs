//! Parses a public submission body into raw fields. The body comes from
//! anyone on the internet, so this is pure, bounded and fuzzed: it never
//! panics and never returns more than [`LIMITS`] allows.

use std::collections::BTreeMap;

/// Field name to its values, in body order. Urlencoded bodies repeat a key
/// for several values; JSON uses an array.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawFields(pub BTreeMap<String, Vec<String>>);

/// The body encodings ingest accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// `application/x-www-form-urlencoded`, what a plain HTML form posts.
    UrlEncoded,
    /// `application/json`: one object.
    Json,
}

/// Why a body was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The content type is neither urlencoded nor JSON.
    #[error("the body must be application/x-www-form-urlencoded or application/json")]
    UnsupportedType,
    /// The body is over [`Limits::body_bytes`].
    #[error("the body must be at most {} bytes", LIMITS.body_bytes)]
    TooLarge,
    /// The body is not valid for its content type.
    #[error("the body is malformed: {0}")]
    Malformed(String),
    /// More distinct fields than [`Limits::fields`].
    #[error("the body has more than {} fields", LIMITS.fields)]
    TooManyFields,
    /// A field name is over [`Limits::key_bytes`].
    #[error("a field name is longer than {} bytes", LIMITS.key_bytes)]
    KeyTooLong,
    /// A value is over [`Limits::value_chars`].
    #[error("field {0} has a value longer than {max} characters", max = LIMITS.value_chars)]
    ValueTooLong(String),
    /// A field has more than [`Limits::values_per_key`] values.
    #[error("field {0} has more than {max} values", max = LIMITS.values_per_key)]
    TooManyValues(String),
    /// A JSON value is an object or a nested array.
    #[error("field {0} must be a string, number, boolean or an array of them")]
    NestedValue(String),
}

/// Bounds on what one submission may carry.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The whole body.
    pub body_bytes: usize,
    /// Distinct field names.
    pub fields: usize,
    /// One field name, in bytes.
    pub key_bytes: usize,
    /// One value, in characters.
    pub value_chars: usize,
    /// Values for one field name.
    pub values_per_key: usize,
}

/// The limits ingest enforces.
pub const LIMITS: Limits = Limits {
    body_bytes: 64 * 1024,
    fields: 100,
    key_bytes: 128,
    value_chars: 10_000,
    values_per_key: 50,
};

/// The body kind for a `Content-Type` header value. Parameters such as
/// `charset` are ignored.
///
/// # Errors
///
/// `UnsupportedType` when the header is missing or names another type.
pub fn body_kind(content_type: Option<&str>) -> Result<BodyKind, ParseError> {
    let essence = content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .ok_or(ParseError::UnsupportedType)?;
    if essence.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
        Ok(BodyKind::UrlEncoded)
    } else if essence.eq_ignore_ascii_case("application/json") {
        Ok(BodyKind::Json)
    } else {
        Err(ParseError::UnsupportedType)
    }
}

/// Parses `body` as `kind`. Empty field names are skipped. JSON numbers and
/// booleans become their JSON text and `null` is skipped, so both encodings
/// store the same strings.
///
/// # Errors
///
/// When the body is malformed or breaks a [`LIMITS`] bound.
pub fn parse(kind: BodyKind, body: &[u8]) -> Result<RawFields, ParseError> {
    if body.len() > LIMITS.body_bytes {
        return Err(ParseError::TooLarge);
    }
    let mut fields = Collector::default();
    match kind {
        BodyKind::UrlEncoded => {
            let text = std::str::from_utf8(body)
                .map_err(|_| ParseError::Malformed("the body is not UTF-8".to_owned()))?;
            for (key, value) in url::form_urlencoded::parse(text.as_bytes()) {
                fields.push(&key, value.into_owned())?;
            }
        }
        BodyKind::Json => {
            let document: serde_json::Value = serde_json::from_slice(body)
                .map_err(|err| ParseError::Malformed(err.to_string()))?;
            let serde_json::Value::Object(object) = document else {
                return Err(ParseError::Malformed("the body must be a JSON object".to_owned()));
            };
            for (key, value) in object {
                match value {
                    serde_json::Value::Array(items) => {
                        for item in items {
                            if let Some(text) = scalar(&key, item)? {
                                fields.push(&key, text)?;
                            }
                        }
                    }
                    other @ (serde_json::Value::Null
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::String(_)
                    | serde_json::Value::Object(_)) => {
                        if let Some(text) = scalar(&key, other)? {
                            fields.push(&key, text)?;
                        }
                    }
                }
            }
        }
    }
    Ok(RawFields(fields.0))
}

fn scalar(key: &str, value: serde_json::Value) -> Result<Option<String>, ParseError> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(text) => Ok(Some(text)),
        serde_json::Value::Bool(flag) => Ok(Some(flag.to_string())),
        serde_json::Value::Number(number) => Ok(Some(number.to_string())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Err(ParseError::NestedValue(key.to_owned()))
        }
    }
}

#[derive(Debug, Default)]
struct Collector(BTreeMap<String, Vec<String>>);

impl Collector {
    fn push(&mut self, key: &str, value: String) -> Result<(), ParseError> {
        if key.is_empty() {
            return Ok(());
        }
        if key.len() > LIMITS.key_bytes {
            return Err(ParseError::KeyTooLong);
        }
        if value.chars().count() > LIMITS.value_chars {
            return Err(ParseError::ValueTooLong(key.to_owned()));
        }
        if !self.0.contains_key(key) && self.0.len() >= LIMITS.fields {
            return Err(ParseError::TooManyFields);
        }
        let values = self.0.entry(key.to_owned()).or_default();
        if values.len() >= LIMITS.values_per_key {
            return Err(ParseError::TooManyValues(key.to_owned()));
        }
        values.push(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn fields(pairs: &[(&str, &[&str])]) -> RawFields {
        RawFields(
            pairs
                .iter()
                .map(|(key, values)| {
                    ((*key).to_owned(), values.iter().map(|v| (*v).to_owned()).collect())
                })
                .collect(),
        )
    }

    #[test]
    fn content_types() {
        assert_eq!(
            body_kind(Some("application/x-www-form-urlencoded; charset=UTF-8")),
            Ok(BodyKind::UrlEncoded),
            "urlencoded with charset"
        );
        assert_eq!(body_kind(Some("Application/JSON")), Ok(BodyKind::Json), "json, any case");
        assert_eq!(
            body_kind(Some("multipart/form-data; boundary=x")),
            Err(ParseError::UnsupportedType),
            "multipart"
        );
        assert_eq!(body_kind(None), Err(ParseError::UnsupportedType), "missing");
    }

    #[test]
    fn urlencoded_repeats_keys() {
        let parsed =
            parse(BodyKind::UrlEncoded, b"name=Ada+L&topic=a&topic=b&=skip&email=ada%40x.io")
                .unwrap();
        assert_eq!(
            parsed,
            fields(&[("email", &["ada@x.io"]), ("name", &["Ada L"]), ("topic", &["a", "b"])]),
            "parsed fields"
        );
    }

    #[test]
    fn json_scalars_become_text() {
        let body = br#"{"n": 3.5, "ok": true, "none": null, "tags": ["a", 1], "name": "Ada"}"#;
        let parsed = parse(BodyKind::Json, body).unwrap();
        assert_eq!(
            parsed,
            fields(&[
                ("n", &["3.5"]),
                ("name", &["Ada"]),
                ("ok", &["true"]),
                ("tags", &["a", "1"])
            ]),
            "parsed fields"
        );
    }

    #[test]
    fn json_rejects_nesting_and_non_objects() {
        assert_eq!(
            parse(BodyKind::Json, br#"{"a": {"b": 1}}"#),
            Err(ParseError::NestedValue("a".to_owned())),
            "object value"
        );
        assert_eq!(
            parse(BodyKind::Json, br#"{"a": [[1]]}"#),
            Err(ParseError::NestedValue("a".to_owned())),
            "nested array"
        );
        assert!(
            matches!(parse(BodyKind::Json, b"[1]"), Err(ParseError::Malformed(_))),
            "array body"
        );
        assert!(matches!(parse(BodyKind::Json, b"{"), Err(ParseError::Malformed(_))), "truncated");
    }

    #[test]
    fn limits_hold() {
        let big = vec![b'a'; LIMITS.body_bytes + 1];
        assert_eq!(parse(BodyKind::UrlEncoded, &big), Err(ParseError::TooLarge), "body size");

        let many = (0..=LIMITS.fields).map(|i| format!("k{i}=v")).collect::<Vec<_>>().join("&");
        assert_eq!(
            parse(BodyKind::UrlEncoded, many.as_bytes()),
            Err(ParseError::TooManyFields),
            "fields"
        );

        let repeated = "k=v&".repeat(LIMITS.values_per_key + 1);
        assert_eq!(
            parse(BodyKind::UrlEncoded, repeated.as_bytes()),
            Err(ParseError::TooManyValues("k".to_owned())),
            "values per key"
        );

        let long_key = format!("{}=v", "k".repeat(LIMITS.key_bytes + 1));
        assert_eq!(
            parse(BodyKind::UrlEncoded, long_key.as_bytes()),
            Err(ParseError::KeyTooLong),
            "key"
        );

        let long_value = format!("k={}", "v".repeat(LIMITS.value_chars + 1));
        assert_eq!(
            parse(BodyKind::UrlEncoded, long_value.as_bytes()),
            Err(ParseError::ValueTooLong("k".to_owned())),
            "value"
        );
    }

    #[test]
    fn urlencoded_must_be_utf8() {
        assert!(
            matches!(parse(BodyKind::UrlEncoded, b"a=\xff"), Err(ParseError::Malformed(_))),
            "raw invalid UTF-8"
        );
    }

    /// The fuzz harness: under `cargo test` it runs as a property test, and
    /// `mise run fuzz` runs it under libFuzzer.
    #[test]
    fn fuzz_parse() {
        bolero::check!().with_type::<(bool, Vec<u8>)>().for_each(|(json, body)| {
            let kind = if *json { BodyKind::Json } else { BodyKind::UrlEncoded };
            if let Ok(RawFields(map)) = parse(kind, body) {
                assert!(map.len() <= LIMITS.fields, "field count");
                for (key, values) in &map {
                    assert!(!key.is_empty() && key.len() <= LIMITS.key_bytes, "key bounds");
                    assert!(!values.is_empty() && values.len() <= LIMITS.values_per_key, "values");
                    assert!(
                        values.iter().all(|v| v.chars().count() <= LIMITS.value_chars),
                        "value bounds"
                    );
                }
            }
        });
    }

    fn field_map() -> impl Strategy<Value = BTreeMap<String, Vec<String>>> {
        proptest::collection::btree_map(
            "[a-z_][a-z0-9_-]{0,15}",
            proptest::collection::vec("\\PC{0,30}", 1..4),
            0..10,
        )
    }

    proptest! {
        #[test]
        fn both_encodings_agree(map in field_map()) {
            let mut urlencoded = url::form_urlencoded::Serializer::new(String::new());
            for (key, values) in &map {
                for value in values {
                    urlencoded.append_pair(key, value);
                }
            }
            let urlencoded = urlencoded.finish();
            let json = serde_json::to_vec(&map).unwrap();
            let from_form = parse(BodyKind::UrlEncoded, urlencoded.as_bytes()).unwrap();
            let from_json = parse(BodyKind::Json, &json).unwrap();
            prop_assert_eq!(&from_form, &from_json, "encodings disagree");
            prop_assert_eq!(from_form.0, map, "round trip");
        }
    }
}
