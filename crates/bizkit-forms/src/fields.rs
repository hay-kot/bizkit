//! Field definitions: checking a form's definitions, and checking a
//! submission against them.

use std::collections::{BTreeMap, BTreeSet};

use bizkit_core::auth::is_plausible_email;
use bizkit_core::validate::FieldErrors;

use crate::api::{FieldDef, FieldKind, FieldValue};
use crate::ingest::bot::ALTCHA_FIELD;
use crate::ingest::parse::RawFields;

/// The most fields a form may define.
pub const MAX_FIELDS: usize = 50;
/// The default and largest `max_length`.
pub const MAX_LENGTH: u32 = 10_000;
const MAX_OPTIONS: usize = 50;

/// True for fields ingest uses itself and never stores: anything starting
/// with `_` (such as the `_gotcha` honeypot) and the `altcha` proof.
#[must_use]
pub fn is_control(name: &str) -> bool {
    name.starts_with('_') || name == ALTCHA_FIELD
}

/// Checks a form's field definitions, recording problems under `path`
/// (`fields[2].name`).
pub fn check_defs(errors: &mut FieldErrors, path: &str, defs: &[FieldDef]) {
    if defs.len() > MAX_FIELDS {
        errors.add(path, format!("must have at most {MAX_FIELDS} fields"));
    }
    let mut seen = BTreeSet::new();
    for (i, def) in defs.iter().enumerate() {
        let at = |field: &str| format!("{path}[{i}].{field}");
        let valid_name = (1..=64).contains(&def.name.len())
            && def.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            && !is_control(&def.name);
        if !valid_name {
            errors.add(
                at("name"),
                "must be 1 to 64 letters, digits, _ or -, not starting with _ and not altcha",
            );
        } else if !seen.insert(def.name.as_str()) {
            errors.add(at("name"), "is used by another field");
        }
        if def.label.as_ref().is_some_and(|label| label.chars().count() > 120) {
            errors.add(at("label"), "must be at most 120 characters");
        }
        if def.max_length.is_some_and(|max| !(1..=MAX_LENGTH).contains(&max)) {
            errors.add(at("max_length"), format!("must be between 1 and {MAX_LENGTH}"));
        }
        match def.kind {
            FieldKind::Choice => {
                if def.options.is_empty() || def.options.len() > MAX_OPTIONS {
                    errors.add(at("options"), format!("must have 1 to {MAX_OPTIONS} options"));
                }
                if def.options.iter().any(|o| o.trim().is_empty() || o.chars().count() > 200) {
                    errors.add(at("options"), "must each be 1 to 200 characters");
                }
            }
            FieldKind::Text | FieldKind::Email | FieldKind::Url | FieldKind::Number => {
                if !def.options.is_empty() || def.multiple {
                    errors.add(at("options"), "only choice fields take options or multiple");
                }
            }
        }
    }
}

/// The values of every non-control field, trimmed, blanks dropped. Used for
/// forms without definitions and for spam, which skips validation.
#[must_use]
pub fn keep_all(raw: RawFields) -> BTreeMap<String, FieldValue> {
    raw.0
        .into_iter()
        .filter(|(name, _)| !is_control(name))
        .filter_map(|(name, values)| {
            let mut values = non_blank(values);
            match values.len() {
                0 => None,
                1 => values.pop().map(|value| (name, FieldValue::One(value))),
                _ => Some((name, FieldValue::Many(values))),
            }
        })
        .collect()
}

/// Checks a submission against `defs`. With no definitions every
/// non-control field is kept; otherwise unknown fields are dropped.
///
/// # Errors
///
/// Every problem, keyed by field name.
pub fn validate(
    defs: &[FieldDef],
    raw: RawFields,
) -> Result<BTreeMap<String, FieldValue>, FieldErrors> {
    if defs.is_empty() {
        return Ok(keep_all(raw));
    }
    let mut raw = raw.0;
    let mut errors = FieldErrors::default();
    let mut data = BTreeMap::new();
    for def in defs {
        let values = non_blank(raw.remove(&def.name).unwrap_or_default());
        if values.is_empty() {
            if def.required {
                errors.add(&def.name, "is required");
            }
            continue;
        }
        let takes_many = def.kind == FieldKind::Choice && def.multiple;
        if values.len() > 1 && !takes_many {
            errors.add(&def.name, "takes one value");
            continue;
        }
        let max = usize::try_from(def.max_length.unwrap_or(MAX_LENGTH)).unwrap_or(usize::MAX);
        for value in &values {
            if value.chars().count() > max {
                errors.add(&def.name, format!("must be at most {max} characters"));
            } else if let Some(problem) = check_value(def, value) {
                errors.add(&def.name, problem);
            }
        }
        let value = if takes_many {
            FieldValue::Many(values)
        } else {
            FieldValue::One(values.into_iter().next().unwrap_or_default())
        };
        data.insert(def.name.clone(), value);
    }
    errors.into_result().map(|()| data)
}

fn non_blank(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).collect()
}

fn check_value(def: &FieldDef, value: &str) -> Option<&'static str> {
    match def.kind {
        FieldKind::Text => None,
        FieldKind::Email => (!is_plausible_email(value)).then_some("must be an email address"),
        FieldKind::Url => {
            let ok =
                url::Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"));
            (!ok).then_some("must be an http or https URL")
        }
        FieldKind::Number => {
            let ok = value.parse::<f64>().is_ok_and(f64::is_finite);
            (!ok).then_some("must be a number")
        }
        FieldKind::Choice => {
            (!def.options.iter().any(|o| o == value)).then_some("must be one of the options")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, kind: FieldKind) -> FieldDef {
        FieldDef {
            name: name.to_owned(),
            label: None,
            kind,
            required: false,
            max_length: None,
            options: Vec::new(),
            multiple: false,
        }
    }

    fn raw(pairs: &[(&str, &[&str])]) -> RawFields {
        RawFields(
            pairs
                .iter()
                .map(|(k, vs)| ((*k).to_owned(), vs.iter().map(|v| (*v).to_owned()).collect()))
                .collect(),
        )
    }

    #[test]
    fn no_definitions_keeps_everything_but_control_fields() {
        let data = validate(
            &[],
            raw(&[
                ("name", &[" Ada "]),
                ("tags", &["a", "b"]),
                ("_gotcha", &[""]),
                ("altcha", &["x"]),
                ("blank", &[" "]),
            ]),
        )
        .unwrap();
        assert_eq!(
            data,
            BTreeMap::from([
                ("name".to_owned(), FieldValue::One("Ada".to_owned())),
                ("tags".to_owned(), FieldValue::Many(vec!["a".to_owned(), "b".to_owned()])),
            ]),
            "kept fields"
        );
    }

    #[test]
    fn definitions_check_and_drop_unknown_fields() {
        let mut topic = def("topic", FieldKind::Choice);
        topic.options = vec!["sales".to_owned(), "support".to_owned()];
        topic.multiple = true;
        let mut email = def("email", FieldKind::Email);
        email.required = true;
        let defs = [email, def("site", FieldKind::Url), def("seats", FieldKind::Number), topic];

        let data = validate(
            &defs,
            raw(&[
                ("email", &["a@b.io"]),
                ("seats", &["3"]),
                ("topic", &["sales"]),
                ("submit", &["Send"]),
            ]),
        )
        .unwrap();
        assert_eq!(data.keys().collect::<Vec<_>>(), ["email", "seats", "topic"], "kept fields");
        assert_eq!(data["topic"], FieldValue::Many(vec!["sales".to_owned()]), "multiple choice");

        let errors = validate(
            &defs,
            raw(&[("site", &["ftp://x"]), ("seats", &["inf"]), ("topic", &["other"])]),
        )
        .unwrap_err();
        assert_eq!(
            errors.as_map().iter().map(|(k, v)| (k.as_str(), v[0].as_str())).collect::<Vec<_>>(),
            [
                ("email", "is required"),
                ("seats", "must be a number"),
                ("site", "must be an http or https URL"),
                ("topic", "must be one of the options"),
            ],
            "errors"
        );
    }

    #[test]
    fn single_value_fields_reject_repeats_and_long_values() {
        let mut name = def("name", FieldKind::Text);
        name.max_length = Some(3);
        let too_long = validate(&[name.clone()], raw(&[("name", &["abcd"])])).unwrap_err();
        assert_eq!(too_long.as_map()["name"], ["must be at most 3 characters"], "too long");
        let repeated = validate(&[name], raw(&[("name", &["a", "b"])])).unwrap_err();
        assert_eq!(repeated.as_map()["name"], ["takes one value"], "repeated");
    }

    #[test]
    fn definition_checks() {
        let mut choice = def("pick", FieldKind::Choice);
        choice.max_length = Some(0);
        let mut text = def("note", FieldKind::Text);
        text.multiple = true;
        let defs = [
            def("_x", FieldKind::Text),
            def("a", FieldKind::Text),
            def("a", FieldKind::Text),
            choice,
            text,
        ];
        let mut errors = FieldErrors::default();
        check_defs(&mut errors, "fields", &defs);
        assert_eq!(
            errors.as_map().keys().collect::<Vec<_>>(),
            [
                "fields[0].name",
                "fields[2].name",
                "fields[3].max_length",
                "fields[3].options",
                "fields[4].options"
            ],
            "error paths"
        );
    }
}
