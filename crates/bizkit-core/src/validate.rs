//! Input validation. Deserialization errors and [`Validate`] errors share one
//! shape: messages per field path.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;

/// Messages per input field. Keys are paths such as `items[2].name`; an empty
/// key means the input as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldErrors(BTreeMap<String, Vec<String>>);

impl FieldErrors {
    /// Records `message` against `field`.
    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.0.entry(field.into()).or_default().push(message.into());
    }

    /// True when nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `Ok` when nothing was recorded, so `validate` can end with `errors.into_result()`.
    ///
    /// # Errors
    ///
    /// `self` when it holds any messages.
    pub fn into_result(self) -> Result<(), Self> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }

    /// The messages per field.
    #[must_use]
    pub const fn as_map(&self) -> &BTreeMap<String, Vec<String>> {
        &self.0
    }
}

/// Checks an operation's input after it deserializes.
pub trait Validate {
    /// Returns every problem with the input at once.
    ///
    /// # Errors
    ///
    /// Every problem with the input.
    fn validate(&self) -> Result<(), FieldErrors> {
        Ok(())
    }
}

/// Deserializes `value`, reporting a failure as a field error at its path.
///
/// # Errors
///
/// A field error at the path where deserialization failed.
pub fn from_json<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, FieldErrors> {
    serde_path_to_error::deserialize(value).map_err(|err| {
        let path = err.path().to_string();
        let inner = err.into_inner().to_string();
        let mut errors = FieldErrors::default();
        match missing_field(&inner) {
            Some(field) => {
                let full = if path == "." { field.to_owned() } else { format!("{path}.{field}") };
                errors.add(full, "is required");
            }
            None => errors.add(if path == "." { String::new() } else { path }, inner),
        }
        errors
    })
}

/// serde reports a missing field at the parent's path; this pulls the name out
/// of its message so the error lands on the field itself.
fn missing_field(message: &str) -> Option<&str> {
    message.strip_prefix("missing field `")?.split('`').next()
}

/// Checks a URL-safe slug: 2-48 characters of `a-z`, `0-9` and `-`, starting
/// with a letter and not ending with `-`.
pub fn check_slug(errors: &mut FieldErrors, field: &str, slug: &str) {
    let valid_chars =
        slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    let starts_with_letter = slug.bytes().next().is_some_and(|b| b.is_ascii_lowercase());
    if !(2..=48).contains(&slug.len()) {
        errors.add(field, "must be 2 to 48 characters");
    } else if !valid_chars || !starts_with_letter || slug.ends_with('-') {
        errors.add(field, "must be lowercase letters, digits and dashes, starting with a letter");
    }
}

/// Checks a display name: 1-120 characters after trimming.
pub fn check_name(errors: &mut FieldErrors, field: &str, name: &str) {
    let len = name.trim().chars().count();
    if len == 0 {
        errors.add(field, "must not be empty");
    } else if len > 120 {
        errors.add(field, "must be at most 120 characters");
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "fields exist to be deserialized")]
    struct Input {
        name: String,
        items: Vec<Item>,
    }

    #[derive(Debug, Deserialize)]
    #[expect(dead_code, reason = "fields exist to be deserialized")]
    struct Item {
        count: u32,
    }

    #[test]
    fn type_errors_report_the_path() {
        let value = serde_json::json!({"name": "a", "items": [{"count": 1}, {"count": "x"}]});
        let errors = from_json::<Input>(value).unwrap_err();
        assert_eq!(errors.as_map().keys().collect::<Vec<_>>(), ["items[1].count"], "path");
    }

    #[test]
    fn missing_fields_report_the_field() {
        let errors = from_json::<Input>(serde_json::json!({"items": []})).unwrap_err();
        assert_eq!(errors.as_map()["name"], ["is required"], "missing field");
    }

    #[test]
    fn slugs() {
        for good in ["ab", "acme", "a-1", "team-w0r0abc123"] {
            let mut errors = FieldErrors::default();
            check_slug(&mut errors, "slug", good);
            assert!(errors.is_empty(), "{good} should be a valid slug");
        }
        for bad in ["a", "1a", "a-", "Acme", "a_b", "a b", &"a".repeat(49)] {
            let mut errors = FieldErrors::default();
            check_slug(&mut errors, "slug", bad);
            assert!(!errors.is_empty(), "{bad} should be an invalid slug");
        }
    }
}
