//! Keeps secret values out of stored runs and job errors.

use std::collections::BTreeMap;

/// Values shorter than this are left alone; redacting them would mostly hit
/// unrelated text.
pub(crate) const MIN_SECRET_CHARS: usize = 4;

/// Replaces each secret value in `text` with `[secret:NAME]`. Longer values
/// go first, so a secret that contains another is replaced whole.
#[derive(Debug, Clone, Default)]
pub(crate) struct Redactor {
    secrets: Vec<(String, String)>,
}

impl Redactor {
    pub(crate) fn new(secrets: &BTreeMap<String, String>) -> Self {
        let mut secrets: Vec<(String, String)> = secrets
            .iter()
            .filter(|(_, value)| value.chars().count() >= MIN_SECRET_CHARS)
            .map(|(name, value)| (value.clone(), format!("[secret:{name}]")))
            .collect();
        secrets.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
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
}
