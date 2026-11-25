//! A signer's signature or initials: strokes drawn on the pad, or typed.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The pad's coordinate space: strokes are points in 0..=1000 by 0..=300.
pub(crate) const PAD_WIDTH: u16 = 1000;
pub(crate) const PAD_HEIGHT: u16 = 300;
const MAX_STROKES: usize = 50;
const MAX_POINTS: usize = 2000;
const MAX_TYPED: usize = 100;

/// How a signer signed or initialed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[schema(as = SigningSignature)]
pub(crate) enum Signature {
    /// Strokes drawn on a 1000 by 300 pad, each a list of `[x, y]` points
    /// with the origin top left; at most 50 strokes and 2000 points.
    Drawn {
        /// The strokes.
        #[schema(value_type = Vec<Vec<Vec<u16>>>)]
        strokes: Vec<Vec<[u16; 2]>>,
    },
    /// A typed name or initials: 1 to 100 characters the PDF's standard
    /// font can print (Latin letters).
    Typed {
        /// The text.
        name: String,
    },
}

impl Signature {
    /// How the overlay draws it in a field.
    pub(crate) fn mark(&self) -> crate::overlay::Mark<'_> {
        match self {
            Self::Drawn { strokes } => crate::overlay::Mark::Strokes(strokes),
            Self::Typed { name } => crate::overlay::Mark::Typed(name),
        }
    }

    /// The signature as it is stored: empty strokes dropped and typed text
    /// trimmed. The error is a message for the signer.
    pub(crate) fn checked(self) -> Result<Self, &'static str> {
        match self {
            Self::Drawn { strokes } => Self::drawn(strokes),
            Self::Typed { name } => Self::typed(&name),
        }
    }

    fn drawn(strokes: Vec<Vec<[u16; 2]>>) -> Result<Self, &'static str> {
        let strokes: Vec<Vec<[u16; 2]>> =
            strokes.into_iter().filter(|stroke| !stroke.is_empty()).collect();
        let points: usize = strokes.iter().map(Vec::len).sum();
        // A drawing of single-point strokes draws nothing: each stroke is a
        // path, and one point is no line.
        if !strokes.iter().any(|stroke| stroke.len() >= 2) {
            return Err("must have a drawing, or be typed instead");
        }
        if strokes.len() > MAX_STROKES || points > MAX_POINTS {
            return Err("must be a simpler drawing");
        }
        let inside = strokes.iter().flatten().all(|[x, y]| *x <= PAD_WIDTH && *y <= PAD_HEIGHT);
        if !inside {
            return Err("must stay inside the pad");
        }
        Ok(Self::Drawn { strokes })
    }

    fn typed(name: &str) -> Result<Self, &'static str> {
        let name = name.trim();
        let len = name.chars().count();
        if len == 0 {
            return Err("must not be empty");
        }
        if len > MAX_TYPED {
            return Err("must be at most 100 characters");
        }
        if !name.chars().all(|c| crate::pdf::win_ansi(c).is_some()) {
            return Err("must use Latin letters when typed, or be drawn instead");
        }
        Ok(Self::Typed { name: name.to_owned() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(json: &str) -> Result<Signature, &'static str> {
        serde_json::from_str::<Signature>(&format!(r#"{{"kind":"drawn","strokes":{json}}}"#))
            .map_err(|_| "not JSON")
            .and_then(Signature::checked)
    }

    fn typed(name: &str) -> Result<Signature, &'static str> {
        Signature::Typed { name: name.to_owned() }.checked()
    }

    #[test]
    fn drawn_signatures_need_points_inside_the_pad() {
        assert_eq!(
            drawn("[[[1,2],[30,40]],[]]"),
            Ok(Signature::Drawn { strokes: vec![vec![[1, 2], [30, 40]]] })
        );
        assert!(drawn("[]").is_err());
        assert!(drawn("[[[1,2]]]").is_err(), "one point is a dot");
        assert!(drawn("[[[1,2]],[[30,40]]]").is_err(), "dots alone draw no line");
        assert!(drawn("[[[1,2]],[[30,40],[50,60]]]").is_ok(), "one line is enough");
        assert!(drawn("[[[1,2],[1001,4]]]").is_err());
        let many = format!("[{}]", vec!["[[1,1],[2,2]]"; 51].join(","));
        assert!(drawn(&many).is_err(), "too many strokes");
        assert!(
            serde_json::from_str::<Signature>(r#"{"kind":"drawn","strokes":{"x":1}}"#).is_err()
        );
    }

    #[test]
    fn typed_signatures_must_print_in_the_standard_font() {
        assert_eq!(
            typed("  José Müller "),
            Ok(Signature::Typed { name: "José Müller".to_owned() })
        );
        assert!(typed(" ").is_err());
        assert!(typed("Пётр").is_err());
        assert!(serde_json::from_str::<Signature>(r#"{"kind":"other","name":"Ada"}"#).is_err());
    }
}
