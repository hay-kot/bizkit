//! The mail sent for each new inbox submission.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bizkit_core::deps::Mail;
use url::Url;

use crate::api::{FieldDef, FieldValue};

/// The notification for one submission to `to`. Fields print in definition
/// order with their labels; forms without definitions print by name.
#[must_use]
pub fn mail(
    form_name: &str,
    defs: &[FieldDef],
    data: &BTreeMap<String, FieldValue>,
    link: &Url,
    to: &str,
) -> Mail {
    let mut text = format!("{form_name} has a new submission.\n\n");
    let mut add_line = |label: &str, value: &FieldValue| {
        // Writing to a String can't fail.
        let _ignored = writeln!(text, "{label}: {}", value.values().join(", "));
    };
    if defs.is_empty() {
        for (name, value) in data {
            add_line(name, value);
        }
    } else {
        for def in defs {
            if let Some(value) = data.get(&def.name) {
                add_line(def.label.as_deref().unwrap_or(&def.name), value);
            }
        }
    }
    let _ignored = write!(text, "\nSee every submission: {link}\n");
    Mail {
        to: to.to_owned(),
        subject: format!("New submission: {form_name}"),
        text,
        html: None,
        unsubscribe: None,
        id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::FieldKind;

    #[test]
    fn lists_fields_with_labels() {
        let defs = [
            FieldDef {
                name: "email".to_owned(),
                label: Some("Email".to_owned()),
                kind: FieldKind::Email,
                required: true,
                max_length: None,
                options: Vec::new(),
                multiple: false,
            },
            FieldDef {
                name: "topic".to_owned(),
                label: None,
                kind: FieldKind::Choice,
                required: false,
                max_length: None,
                options: vec!["a".to_owned(), "b".to_owned()],
                multiple: true,
            },
        ];
        let data = BTreeMap::from([
            ("topic".to_owned(), FieldValue::Many(vec!["a".to_owned(), "b".to_owned()])),
            ("email".to_owned(), FieldValue::One("ada@example.com".to_owned())),
        ]);
        let link = Url::parse("https://hub.example.com/w/ws/p/p/forms/f").unwrap();
        let mail = mail("Contact", &defs, &data, &link, "owner@example.com");
        assert_eq!(mail.subject, "New submission: Contact", "subject");
        assert_eq!(mail.to, "owner@example.com", "recipient");
        insta::assert_snapshot!(mail.text, @r"
        Contact has a new submission.

        Email: ada@example.com
        topic: a, b

        See every submission: https://hub.example.com/w/ws/p/p/forms/f
        ");
    }
}
