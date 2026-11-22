//! Submissions as CSV (RFC 4180, CRLF line ends).

use std::collections::BTreeSet;

use crate::api::{FieldDef, Submission};

/// The most rows one export holds.
pub const MAX_ROWS: usize = 10_000;
/// The most CSV one export holds, in bytes.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

/// The CSV for `submissions`, and whether rows were left out.
///
/// Rows stop before the text passes [`MAX_BYTES`]. Columns are `id`, `created_at`, `status`, the defined
/// field names, then the sorted keys the data has beyond them, so removed
/// or renamed fields stay exportable.
#[must_use]
pub fn csv(defs: &[FieldDef], submissions: &[Submission]) -> (String, bool) {
    let mut columns: Vec<String> = defs.iter().map(|def| def.name.clone()).collect();
    let extra: BTreeSet<&String> = submissions
        .iter()
        .flat_map(|s| s.data.keys())
        .filter(|key| !columns.contains(key))
        .collect();
    columns.extend(extra.into_iter().cloned());
    let mut out = String::new();
    let header = ["id", "created_at", "status"]
        .into_iter()
        .map(str::to_owned)
        .chain(columns.iter().cloned());
    push_row(&mut out, header);
    for submission in submissions {
        let before = out.len();
        let created = submission
            .created_at
            .to_rfc3339()
            .unwrap_or_else(|_| submission.created_at.0.to_string());
        let fixed = [submission.id.to_string(), created, submission.status.as_str().to_owned()];
        let values = columns.iter().map(|column| {
            submission.data.get(column).map_or_else(String::new, |value| value.values().join("; "))
        });
        push_row(&mut out, fixed.into_iter().chain(values));
        if out.len() > MAX_BYTES {
            out.truncate(before);
            return (out, true);
        }
    }
    (out, false)
}

fn push_row(out: &mut String, cells: impl Iterator<Item = String>) {
    for (i, cell) in cells.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&escape(&cell));
    }
    out.push_str("\r\n");
}

/// Quotes a cell when needed, and defuses cells a spreadsheet would run as
/// a formula by prefixing `'`. Negative numbers get the prefix too.
fn escape(cell: &str) -> String {
    let risky =
        cell.starts_with(['\t', '\r']) || cell.trim_start().starts_with(['=', '+', '-', '@']);
    let defused = if risky { format!("'{cell}") } else { cell.to_owned() };
    if defused.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", defused.replace('"', "\"\""))
    } else {
        defused
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bizkit_core::time::UnixMillis;
    use proptest::prelude::*;

    use super::*;
    use crate::api::{FieldValue, SubmissionStatus};

    fn submission(data: &[(&str, FieldValue)]) -> Submission {
        Submission {
            id: "0190a5d0-0000-7000-8000-000000000001".parse().unwrap(),
            form_id: "0190a5d0-0000-7000-8000-000000000002".parse().unwrap(),
            status: SubmissionStatus::Inbox,
            spam_reason: None,
            data: data.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
            user_agent: None,
            referer: None,
            created_at: UnixMillis(1_800_000_000_000),
        }
    }

    fn one(text: &str) -> FieldValue {
        FieldValue::One(text.to_owned())
    }

    #[test]
    fn quotes_defuses_and_joins() {
        let rows = [submission(&[
            ("name", one("Ada, \"the\" first")),
            ("note", one("=HYPERLINK(\"x\")")),
            ("tags", FieldValue::Many(vec!["a".to_owned(), "b".to_owned()])),
            ("=evil", one(" @x")),
        ])];
        let (text, truncated) = csv(&[], &rows);
        assert!(!truncated, "small export");
        insta::assert_snapshot!(text.replace("\r\n", "\n"), @r#"
        id,created_at,status,'=evil,name,note,tags
        0190a5d0-0000-7000-8000-000000000001,2027-01-15T08:00:00Z,inbox,' @x,"Ada, ""the"" first","'=HYPERLINK(""x"")",a; b
        "#);
    }

    #[test]
    fn defined_columns_come_first_and_removed_fields_stay() {
        let defs = [FieldDef {
            name: "email".to_owned(),
            label: None,
            kind: crate::api::FieldKind::Email,
            required: false,
            max_length: None,
            options: Vec::new(),
            multiple: false,
        }];
        let rows = [submission(&[("old", one("x")), ("email", one("a@b.io"))])];
        let (text, _) = csv(&defs, &rows);
        assert!(text.starts_with("id,created_at,status,email,old\r\n"), "{text}");
    }

    /// Splits CSV the way a reader would, to check every row has the header's width.
    fn split_rows(text: &str) -> Vec<usize> {
        let mut widths = Vec::new();
        let (mut cells, mut quoted, mut chars) = (1, false, text.chars().peekable());
        while let Some(c) = chars.next() {
            match (c, quoted) {
                ('"', true) if chars.peek() == Some(&'"') => {
                    chars.next();
                }
                ('"', _) => quoted = !quoted,
                (',', false) => cells += 1,
                ('\r', false) if chars.peek() == Some(&'\n') => {
                    chars.next();
                    widths.push(cells);
                    cells = 1;
                }
                _ => {}
            }
        }
        widths
    }

    proptest! {
        #[test]
        fn every_row_is_as_wide_as_the_header(values in proptest::collection::vec("\\PC{0,12}|[,\"\r\n=]{0,4}", 1..6)) {
            let data: BTreeMap<String, FieldValue> =
                values.iter().enumerate().map(|(i, v)| (format!("f{i}"), one(v))).collect();
            let rows = [Submission { data, ..submission(&[]) }];
            let widths = split_rows(&csv(&[], &rows).0);
            prop_assert_eq!(widths.len(), 2, "two rows");
            prop_assert_eq!(widths[0], widths[1], "same width");
        }
    }
}
