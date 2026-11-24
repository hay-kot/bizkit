//! Fields a sender places on the PDF's pages, each for one signer.
//!
//! A field's rect is fractions of the page's visible box (`CropBox`
//! intersected with `MediaBox`) as displayed after `/Rotate`, with a top-left
//! origin: the pdf.js viewport at any scale, divided by its size.

use bizkit_core::time::UnixMillis;
use bizkit_core::validate::FieldErrors;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::SignerId;

bizkit_core::id_type!(FieldId, "The ID of a field on a signature request's pages.");

/// The most fields a request takes.
pub(crate) const MAX_FIELDS: usize = 200;
/// The smallest side of a field, as a fraction of the page.
const MIN_SIDE: f64 = 0.01;
const MAX_LABEL: usize = 100;

/// What a field asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SigningFieldKind)]
pub enum FieldKind {
    /// The signer's adopted signature.
    Signature,
    /// The signer's adopted initials.
    Initials,
    /// The UTC date the signer signed, set by bizkit.
    Date,
    /// One line of text the signer types.
    Text,
    /// A box the signer ticks.
    Checkbox,
}

impl FieldKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Signature => "signature",
            Self::Initials => "initials",
            Self::Date => "date",
            Self::Text => "text",
            Self::Checkbox => "checkbox",
        }
    }

    pub(crate) fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(match text {
            "signature" => Self::Signature,
            "initials" => Self::Initials,
            "date" => Self::Date,
            "text" => Self::Text,
            "checkbox" => Self::Checkbox,
            other => anyhow::bail!("unknown field kind {other}"),
        })
    }

    /// Text and checkbox fields may be left empty; the others are filled
    /// from the signature itself, so they are always required.
    const fn may_be_optional(self) -> bool {
        matches!(self, Self::Text | Self::Checkbox)
    }
}

const fn yes() -> bool {
    true
}

/// A field to place.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SigningFieldInput)]
pub struct FieldInput {
    /// The signer who fills it; one of the request's.
    pub signer_id: SignerId,
    /// What it asks for.
    pub kind: FieldKind,
    /// The page, from 1.
    pub page: u32,
    /// The left edge, as a fraction of the page width from the left.
    pub x: f64,
    /// The top edge, as a fraction of the page height from the top.
    pub y: f64,
    /// The width, as a fraction of the page width; at least 0.01.
    pub width: f64,
    /// The height, as a fraction of the page height; at least 0.01.
    pub height: f64,
    /// Whether the signer must fill it. Only text and checkbox fields may
    /// be optional.
    #[serde(default = "yes")]
    pub required: bool,
    /// A hint for the signer, such as `Company`; at most 100 characters.
    #[serde(default)]
    pub label: String,
}

/// A field on a request's pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = SigningField)]
pub struct Field {
    /// The field ID.
    pub id: FieldId,
    /// The signer who fills it.
    pub signer_id: SignerId,
    /// What it asks for.
    pub kind: FieldKind,
    /// The page, from 1.
    pub page: u32,
    /// The left edge, as a fraction of the page width from the left.
    pub x: f64,
    /// The top edge, as a fraction of the page height from the top.
    pub y: f64,
    /// The width, as a fraction of the page width.
    pub width: f64,
    /// The height, as a fraction of the page height.
    pub height: f64,
    /// Whether the signer must fill it.
    pub required: bool,
    /// A hint for the signer.
    pub label: String,
    /// What the signer filled in, once they signed.
    pub value: Option<FieldValue>,
    /// When the signer filled it.
    pub filled_at: Option<UnixMillis>,
}

/// A filled field's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schema(as = SigningFieldValue)]
pub enum FieldValue {
    /// The adopted signature goes here.
    Signed,
    /// The adopted initials go here.
    Initialed,
    /// The UTC date of the signature.
    Date {
        /// `YYYY-MM-DD`.
        date: String,
    },
    /// Typed text.
    Text {
        /// One line.
        text: String,
    },
    /// A tick box.
    Checkbox {
        /// True when ticked.
        checked: bool,
    },
}

/// Checks what can be checked without the request: the count, each rect,
/// `required` and the label. The request's signers and page count are the
/// caller's to check.
pub(crate) fn check(fields: &[FieldInput], errors: &mut FieldErrors) {
    if fields.len() > MAX_FIELDS {
        errors.add("fields", format!("must have at most {MAX_FIELDS} fields"));
    }
    for (at, field) in fields.iter().enumerate() {
        let path = |name: &str| format!("fields[{at}].{name}");
        if field.page == 0 {
            errors.add(path("page"), "must be at least 1");
        }
        check_rect(field, &path, errors);
        if !field.required && !field.kind.may_be_optional() {
            errors.add(path("required"), "must be true for this kind");
        }
        if field.label.trim().chars().count() > MAX_LABEL {
            errors.add(path("label"), format!("must be at most {MAX_LABEL} characters"));
        }
    }
}

/// The same sums as the table's CHECK constraints, so a rect that passes
/// here never fails there.
fn check_rect(field: &FieldInput, path: &impl Fn(&str) -> String, errors: &mut FieldErrors) {
    let edge = |value: f64| value.is_finite() && (0.0..1.0).contains(&value);
    if !edge(field.x) {
        errors.add(path("x"), "must be at least 0 and less than 1");
    }
    if !edge(field.y) {
        errors.add(path("y"), "must be at least 0 and less than 1");
    }
    if !(field.width.is_finite() && field.width >= MIN_SIDE) {
        errors.add(path("width"), "must be at least 0.01");
    } else if field.x + field.width > 1.0 {
        errors.add(path("width"), "must keep the field on the page");
    }
    if !(field.height.is_finite() && field.height >= MIN_SIDE) {
        errors.add(path("height"), "must be at least 0.01");
    } else if field.y + field.height > 1.0 {
        errors.add(path("height"), "must keep the field on the page");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(x: f64, y: f64, width: f64, height: f64) -> FieldInput {
        FieldInput {
            signer_id: SignerId::new(),
            kind: FieldKind::Signature,
            page: 1,
            x,
            y,
            width,
            height,
            required: true,
            label: String::new(),
        }
    }

    fn problems(fields: &[FieldInput]) -> Vec<String> {
        let mut errors = FieldErrors::default();
        check(fields, &mut errors);
        errors.as_map().keys().cloned().collect()
    }

    #[test]
    fn rects_stay_on_the_page() {
        assert!(problems(&[field(0.0, 0.0, 1.0, 1.0)]).is_empty(), "the whole page");
        assert!(problems(&[field(0.7, 0.2, 0.3, 0.1)]).is_empty(), "touching the right edge");
        assert_eq!(problems(&[field(1.0, 0.0, 0.1, 0.1)]), ["fields[0].width", "fields[0].x"]);
        assert_eq!(problems(&[field(-0.1, 0.0, 0.1, 0.1)]), ["fields[0].x"]);
        assert_eq!(
            problems(&[field(0.5, 0.5, 0.6, 0.005)]),
            ["fields[0].height", "fields[0].width"]
        );
        assert_eq!(
            problems(&[field(f64::NAN, 0.0, f64::INFINITY, 0.1)]),
            ["fields[0].width", "fields[0].x"]
        );
    }

    #[test]
    fn only_text_and_checkboxes_are_optional() {
        let mut signature = field(0.1, 0.1, 0.2, 0.1);
        signature.required = false;
        let mut text = signature.clone();
        text.kind = FieldKind::Text;
        assert_eq!(problems(&[signature, text]), ["fields[0].required"]);
    }

    #[test]
    fn a_request_takes_at_most_200_fields() {
        let many = vec![field(0.1, 0.1, 0.1, 0.1); MAX_FIELDS + 1];
        assert_eq!(problems(&many), ["fields"]);
        let mut zero = field(0.1, 0.1, 0.1, 0.1);
        zero.page = 0;
        zero.label = "x".repeat(101);
        assert_eq!(problems(&[zero]), ["fields[0].label", "fields[0].page"]);
    }

    #[test]
    fn values_tag_their_kind() {
        let value = FieldValue::Date { date: "2026-11-24".to_owned() };
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"kind":"date","date":"2026-11-24"}"#
        );
        assert_eq!(serde_json::to_string(&FieldValue::Signed).unwrap(), r#"{"kind":"signed"}"#);
    }
}
