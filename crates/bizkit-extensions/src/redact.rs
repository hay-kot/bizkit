//! Keeps secret values out of stored runs and job errors.

use std::collections::BTreeMap;

/// Values shorter than this are left alone; redacting them would mostly hit
/// unrelated text.
pub(crate) const MIN_SECRET_CHARS: usize = 4;

/// Replaces each secret value in `text` with `[secret:NAME]`. It also
/// matches the JSON-escaped form (logged objects go through
/// `JSON.stringify`) and the URL-encoded form. Longer values go first, so a
/// secret that contains another is replaced whole.
#[derive(Debug, Clone, Default)]
pub(crate) struct Redactor {
    secrets: Vec<(String, String)>,
}

impl Redactor {
    pub(crate) fn new(secrets: &BTreeMap<String, String>) -> Self {
        let mut secrets: Vec<(String, String)> = secrets
            .iter()
            .filter(|(_, value)| value.chars().count() >= MIN_SECRET_CHARS)
            .flat_map(|(name, value)| {
                let label = format!("[secret:{name}]");
                forms(value).into_iter().map(move |form| (form, label.clone()))
            })
            .collect();
        secrets.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
        secrets.dedup();
        Self { secrets }
    }

    pub(crate) fn text(&self, text: &str) -> String {
        self.secrets
            .iter()
            .fold(text.to_owned(), |text, (value, label)| text.replace(value.as_str(), label))
    }

    /// Redacts the strings inside a JSON value, keys included.
    pub(crate) fn json(&self, value: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        if self.secrets.is_empty() {
            return value;
        }
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|item| self.json(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.into_iter().map(|(key, item)| (self.text(&key), self.json(item))).collect(),
            ),
            other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => other,
        }
    }
}

/// The ways a value can show up in script output.
fn forms(value: &str) -> Vec<String> {
    let escaped = serde_json::to_string(value)
        .ok()
        .and_then(|json| json.get(1..json.len() - 1).map(str::to_owned));
    let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
    let mut forms = vec![value.to_owned()];
    forms.extend(escaped);
    forms.push(encoded.clone());
    forms.push(encoded.replace('+', "%20"));
    forms.sort();
    forms.dedup();
    forms
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn replaces_values_longest_first() {
        let secrets = BTreeMap::from([
            ("TOKEN".to_owned(), "abcd".to_owned()),
            ("URL".to_owned(), "https://x.test/abcd".to_owned()),
            ("PIN".to_owned(), "123".to_owned()),
        ]);
        let redactor = Redactor::new(&secrets);
        assert_eq!(
            redactor.text("posting to https://x.test/abcd with abcd and 123"),
            "posting to [secret:URL] with [secret:TOKEN] and 123",
            "short values stay"
        );
        assert_eq!(
            redactor.json(json!({"abcd": ["abcd", 1, null]})),
            json!({"[secret:TOKEN]": ["[secret:TOKEN]", 1, null]}),
            "json strings and keys"
        );
    }

    #[test]
    fn replaces_escaped_and_encoded_forms() {
        let secrets = BTreeMap::from([("KEY".to_owned(), r#"a "b"\c d&e"#.to_owned())]);
        let redactor = Redactor::new(&secrets);
        assert_eq!(
            redactor.text(r#"{"key":"a \"b\"\\c d&e"}"#),
            r#"{"key":"[secret:KEY]"}"#,
            "JSON-escaped"
        );
        assert_eq!(redactor.text("?k=a+%22b%22%5Cc+d%26e"), "?k=[secret:KEY]", "form-encoded");
        assert_eq!(
            redactor.text("?k=a%20%22b%22%5Cc%20d%26e"),
            "?k=[secret:KEY]",
            "percent-encoded"
        );
    }
}
