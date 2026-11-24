//! A signer's signature: strokes drawn on the pad, or a typed name.

use serde::{Deserialize, Serialize};

/// The pad's coordinate space: strokes are points in 0..=1000 by 0..=300.
pub(crate) const PAD_WIDTH: u16 = 1000;
pub(crate) const PAD_HEIGHT: u16 = 300;
const MAX_STROKES: usize = 50;
const MAX_POINTS: usize = 2000;
const MAX_TYPED: usize = 100;

/// How a signer signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Signature {
    /// Strokes, each a list of `[x, y]` points on the pad.
    Drawn { strokes: Vec<Vec<[u16; 2]>> },
    /// A typed name, in characters the PDF's standard font can print.
    Typed { name: String },
}

impl Signature {
    /// Reads the signing form's fields. The error is a message for the page.
    pub(crate) fn from_form(
        kind: &str,
        strokes: &str,
        typed_name: &str,
    ) -> Result<Self, &'static str> {
        match kind {
            "drawn" => Self::drawn(strokes),
            "typed" => Self::typed(typed_name),
            _ => Err("Choose whether to type or draw your signature."),
        }
    }

    fn drawn(json: &str) -> Result<Self, &'static str> {
        const EMPTY: &str = "Draw your signature in the box, or type your name instead.";
        if json.trim().is_empty() {
            return Err(EMPTY);
        }
        let strokes: Vec<Vec<[u16; 2]>> = serde_json::from_str(json)
            .map_err(|_| "The drawing didn't come through. Clear it and try again.")?;
        let strokes: Vec<Vec<[u16; 2]>> =
            strokes.into_iter().filter(|stroke| !stroke.is_empty()).collect();
        let points: usize = strokes.iter().map(Vec::len).sum();
        if points < 2 {
            return Err(EMPTY);
        }
        if strokes.len() > MAX_STROKES || points > MAX_POINTS {
            return Err("The drawing is too detailed. Clear it and sign more simply.");
        }
        let inside = strokes.iter().flatten().all(|[x, y]| *x <= PAD_WIDTH && *y <= PAD_HEIGHT);
        if !inside {
            return Err("The drawing didn't come through. Clear it and try again.");
        }
        Ok(Self::Drawn { strokes })
    }

    fn typed(name: &str) -> Result<Self, &'static str> {
        let name = name.trim();
        let len = name.chars().count();
        if len == 0 {
            return Err("Type your full name to sign.");
        }
        if len > MAX_TYPED {
            return Err("Your name must be at most 100 characters.");
        }
        if !name.chars().all(|c| crate::pdf::win_ansi(c).is_some()) {
            return Err("Use Latin letters for a typed signature, or draw it instead.");
        }
        Ok(Self::Typed { name: name.to_owned() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawn_signatures_need_points_inside_the_pad() {
        assert_eq!(
            Signature::from_form("drawn", "[[[1,2],[30,40]]]", ""),
            Ok(Signature::Drawn { strokes: vec![vec![[1, 2], [30, 40]]] })
        );
        assert!(Signature::from_form("drawn", "", "").is_err());
        assert!(Signature::from_form("drawn", "[[[1,2]]]", "").is_err(), "one point is a dot");
        assert!(Signature::from_form("drawn", "[[[1,2],[1001,4]]]", "").is_err());
        assert!(Signature::from_form("drawn", "{\"x\":1}", "").is_err());
        let many = format!("[{}]", vec!["[[1,1],[2,2]]"; 51].join(","));
        assert!(Signature::from_form("drawn", &many, "").is_err(), "too many strokes");
    }

    #[test]
    fn typed_signatures_must_print_in_the_standard_font() {
        assert_eq!(
            Signature::from_form("typed", "", "  José Müller "),
            Ok(Signature::Typed { name: "José Müller".to_owned() })
        );
        assert!(Signature::from_form("typed", "", " ").is_err());
        assert!(Signature::from_form("typed", "", "Пётр").is_err());
        assert!(Signature::from_form("other", "", "Ada").is_err());
    }
}
