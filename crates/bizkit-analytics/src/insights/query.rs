//! The insight query: one typed object that the builder, saved insights
//! and the MCP tool share, and the result it gives.

use bizkit_core::time::UnixMillis;
use bizkit_core::validate::FieldErrors;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::api::{Day, Interval};
use crate::beacon::{MAX_PROP_KEY, MAX_PROP_VALUE, PAGEVIEW};
use crate::names;

/// Filters per query.
pub(crate) const MAX_FILTERS: usize = 10;
/// Breakdown values shown before the rest are counted as one row.
pub(crate) const TOP_VALUES: i64 = 10;
/// The per-user bucket that holds this many times and more.
pub(crate) const TIMES_CAP: i64 = 5;

/// What an insight counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InsightKind {
    /// What people did: events over the range.
    Events,
    /// Who people are now: identified people by trait, whatever the range.
    People,
}

/// How an events insight counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Measure {
    /// Matching events.
    #[default]
    Total,
    /// Distinct people who sent them. A cookieless visitor counts once a day.
    Users,
    /// Distinct identified people who sent them, over the active people:
    /// identified or aliased people with any event in the range.
    Share,
    /// Identified people by how many times they sent it: 1, 2, 3, 4, 5+.
    PerUser,
}

/// A filter's comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FilterOp {
    /// Equal: as numbers when both sides are numbers (`2` matches `2.0`),
    /// else as text (`true` matches `"true"`).
    Is,
    /// Not `is`; a missing key matches too.
    IsNot,
    /// A JSON number greater than the value. Text never matches.
    Gt,
    /// A JSON number less than the value. Text never matches.
    Lt,
    /// Present and not null.
    IsSet,
}

/// A filter value: text, a number or a boolean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum FilterValue {
    /// A boolean.
    Bool(bool),
    /// A number.
    Number(f64),
    /// Text. Text that reads as a number also compares as one.
    Text(String),
}

impl FilterValue {
    /// The value as stored text, for text comparison.
    fn as_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Bool(flag) => flag.to_string(),
            // Rust writes whole floats without a point (`2`), as SQLite
            // writes integers; `is` compares numbers as numbers anyway.
            Self::Number(n) => n.to_string(),
        }
    }

    /// The value as a finite number, when it is one or reads as one.
    fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Text(text) => text.trim().parse().ok(),
            Self::Bool(_) => None,
        }
        .filter(|n: &f64| n.is_finite())
    }
}

/// One condition on an event property, or on a trait for the people kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InsightFilter {
    /// The property or trait name.
    pub(crate) key: String,
    /// The comparison.
    pub(crate) op: FilterOp,
    /// The value: text for `is` and `is_not`, a number for `gt` and `lt`,
    /// none for `is_set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<FilterValue>,
}

/// Where a breakdown value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BreakdownSource {
    /// An event property.
    Property,
    /// A trait of the event's person; events without one are "not set".
    Trait,
}

/// Split the result by one property or trait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BreakdownBy {
    /// Where the value comes from.
    pub(crate) source: BreakdownSource,
    /// The property or trait name.
    pub(crate) key: String,
}

/// A typed analytics question. Events insights read raw events, so they
/// reach back only as far as raw retention (90 days by default); keep
/// lifetime counts in traits and ask the people kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct InsightQuery {
    /// Events or people.
    pub(crate) kind: InsightKind,
    /// The event name (`pageview` included). Required for events; absent
    /// for people.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) event: Option<String>,
    /// All must match: event properties for events, traits for people.
    /// At most 10.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) filters: Vec<InsightFilter>,
    /// How events count; default `total`. Absent for people, which count
    /// people.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) measure: Option<Measure>,
    /// Split by one property or trait: the top 10 values, then the rest.
    /// Not with `per_user`; people break down by trait only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) breakdown: Option<BreakdownBy>,
}

impl InsightQuery {
    /// Adds errors under `at` (such as `query`).
    pub(crate) fn check(&self, errors: &mut FieldErrors, at: &str) {
        match self.kind {
            InsightKind::Events => self.check_events(errors, at),
            InsightKind::People => self.check_people(errors, at),
        }
        if self.filters.len() > MAX_FILTERS {
            errors.add(format!("{at}.filters"), format!("must have at most {MAX_FILTERS} filters"));
        }
        for (n, filter) in self.filters.iter().enumerate() {
            check_filter(errors, &format!("{at}.filters[{n}]"), filter);
        }
        if let Some(breakdown) = &self.breakdown {
            check_key(errors, &format!("{at}.breakdown.key"), &breakdown.key);
        }
    }

    fn check_events(&self, errors: &mut FieldErrors, at: &str) {
        match self.event.as_deref() {
            None => errors.add(format!("{at}.event"), "is required for an events insight"),
            Some(name) if name != PAGEVIEW && !names::is_event_name(name) => {
                errors.add(format!("{at}.event"), names::EVENT_NAME_RULE);
            }
            Some(_) => {}
        }
        if self.measure == Some(Measure::PerUser) && self.breakdown.is_some() {
            errors.add(format!("{at}.breakdown"), "can't split a per-user distribution");
        }
    }

    fn check_people(&self, errors: &mut FieldErrors, at: &str) {
        if self.event.is_some() {
            errors.add(format!("{at}.event"), "must be absent for a people insight");
        }
        if self.measure.is_some() {
            errors.add(format!("{at}.measure"), "must be absent: a people insight counts people");
        }
        if self.breakdown.as_ref().is_some_and(|b| b.source != BreakdownSource::Trait) {
            errors.add(format!("{at}.breakdown.source"), "must be trait for a people insight");
        }
    }

    pub(crate) fn measure(&self) -> Measure {
        self.measure.unwrap_or_default()
    }
}

fn check_key(errors: &mut FieldErrors, at: &str, key: &str) {
    if key.is_empty() || key.chars().count() > MAX_PROP_KEY {
        errors.add(at, format!("must be 1-{MAX_PROP_KEY} characters"));
    }
}

fn check_filter(errors: &mut FieldErrors, at: &str, filter: &InsightFilter) {
    check_key(errors, &format!("{at}.key"), &filter.key);
    let value = format!("{at}.value");
    match (filter.op, &filter.value) {
        (FilterOp::IsSet, Some(_)) => errors.add(value, "must be absent for is_set"),
        (FilterOp::IsSet, None) => {}
        (FilterOp::Is | FilterOp::IsNot, None) => errors.add(value, "is required"),
        (FilterOp::Is | FilterOp::IsNot, Some(v)) => {
            if v.as_text().chars().count() > MAX_PROP_VALUE {
                errors.add(value, format!("must be at most {MAX_PROP_VALUE} characters"));
            }
        }
        (FilterOp::Gt | FilterOp::Lt, v) => {
            // `NaN` parses as a float; refusing it keeps `n` set in the SQL.
            if v.as_ref().and_then(FilterValue::as_number).is_none() {
                errors.add(value, "must be a number");
            }
        }
    }
}

/// The filters as the JSON array the SQL reads: `[{k, op, v, n}]`, with
/// `v` the value as stored text and `n` the number.
pub(crate) fn encode_filters(filters: &[InsightFilter]) -> String {
    let items: Vec<Value> = filters
        .iter()
        .map(|filter| {
            let op = match filter.op {
                FilterOp::Is => "is",
                FilterOp::IsNot => "is_not",
                FilterOp::Gt => "gt",
                FilterOp::Lt => "lt",
                FilterOp::IsSet => "is_set",
            };
            let value = filter.value.as_ref();
            json!({
                "k": filter.key,
                "op": op,
                "v": value.map(FilterValue::as_text),
                "n": value.and_then(FilterValue::as_number),
            })
        })
        .collect();
    Value::Array(items).to_string()
}

/// One chart point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct InsightPoint {
    /// When the hour or day starts.
    pub(crate) start: UnixMillis,
    /// The measure: events, people, or a share from 0 to 1.
    pub(crate) value: f64,
}

/// One breakdown value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub(crate) struct InsightRow {
    /// The value as text; none for "not set".
    pub(crate) value: Option<String>,
    /// The measure for this value.
    pub(crate) result: f64,
}

/// People who did the event this many times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct TimesBucket {
    /// The number of times.
    pub(crate) times: i64,
    /// Set on the last bucket, which holds `times` and more.
    pub(crate) or_more: bool,
    /// People in the bucket.
    pub(crate) users: i64,
}

/// An insight's numbers, shaped by how they are best shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "view", rename_all = "snake_case")]
pub(crate) enum InsightResult {
    /// A measure over time: events without a breakdown.
    Series {
        /// The measure.
        measure: Measure,
        /// What one point covers.
        interval: Interval,
        /// The measure over the whole range (a share is people over active
        /// people in the range, not a mean of points).
        value: f64,
        /// For `share`: active people in the range.
        #[serde(default)]
        active: Option<i64>,
        /// One point per hour or day.
        points: Vec<InsightPoint>,
        /// Set when the range starts before the oldest raw events: days
        /// before this one have no data.
        #[serde(default)]
        raw_since: Option<Day>,
    },
    /// The measure split by one property or trait.
    Breakdown {
        /// The measure; none for people.
        #[serde(default)]
        measure: Option<Measure>,
        /// The measure over everything matched.
        value: f64,
        /// For `share`: active people in the range. For people: everyone
        /// known.
        #[serde(default)]
        active: Option<i64>,
        /// The top 10 values, largest first.
        rows: Vec<InsightRow>,
        /// Every other value as one row; distinct measures count each
        /// person once.
        #[serde(default)]
        other: Option<InsightRow>,
        /// As for `series`.
        #[serde(default)]
        raw_since: Option<Day>,
    },
    /// Identified people by how many times they did the event.
    Distribution {
        /// People who did it at least once.
        users: i64,
        /// 1 to 5 times; the last bucket is 5 and more.
        buckets: Vec<TimesBucket>,
        /// As for `series`.
        #[serde(default)]
        raw_since: Option<Day>,
    },
    /// A count of people.
    Count {
        /// People matching the filters.
        people: i64,
        /// Everyone known in the project.
        known: i64,
    },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn errors_of(query: Value) -> Vec<String> {
        let query: InsightQuery = serde_json::from_value(query).unwrap();
        let mut errors = FieldErrors::default();
        query.check(&mut errors, "query");
        errors.as_map().keys().cloned().collect()
    }

    #[test]
    fn valid_queries_pass() {
        assert_eq!(errors_of(json!({"kind": "events", "event": "signup"})), Vec::<String>::new());
        assert_eq!(
            errors_of(json!({"kind": "events", "event": "pageview", "measure": "share"})),
            Vec::<String>::new()
        );
        assert_eq!(
            errors_of(json!({
                "kind": "events", "event": "a", "measure": "users",
                "filters": [{"key": "plan", "op": "is", "value": "pro"},
                            {"key": "n", "op": "gt", "value": 2},
                            {"key": "n", "op": "lt", "value": "3.5"},
                            {"key": "x", "op": "is_set"}],
                "breakdown": {"source": "trait", "key": "plan"}
            })),
            Vec::<String>::new()
        );
        assert_eq!(
            errors_of(json!({
                "kind": "people",
                "filters": [{"key": "api_tokens", "op": "gt", "value": 0}],
                "breakdown": {"source": "trait", "key": "plan"}
            })),
            Vec::<String>::new()
        );
    }

    #[test]
    fn events_rules() {
        assert_eq!(errors_of(json!({"kind": "events"})), ["query.event"]);
        assert_eq!(errors_of(json!({"kind": "events", "event": "a b"})), ["query.event"]);
        assert_eq!(
            errors_of(json!({"kind": "events", "event": "a", "measure": "per_user",
                             "breakdown": {"source": "property", "key": "k"}})),
            ["query.breakdown"]
        );
    }

    #[test]
    fn people_rules() {
        assert_eq!(
            errors_of(json!({"kind": "people", "event": "a", "measure": "total",
                             "breakdown": {"source": "property", "key": "k"}})),
            ["query.breakdown.source", "query.event", "query.measure"]
        );
    }

    #[test]
    fn filter_rules() {
        let filters = json!([
            {"key": "", "op": "is_set"},
            {"key": "a", "op": "is_set", "value": "x"},
            {"key": "a", "op": "is"},
            {"key": "a", "op": "gt", "value": "many"},
            {"key": "a", "op": "lt"},
            {"key": "a", "op": "gt", "value": "NaN"},
            {"key": "a", "op": "is_not", "value": "x".repeat(MAX_PROP_VALUE + 1)},
        ]);
        assert_eq!(
            errors_of(json!({"kind": "events", "event": "e", "filters": filters})),
            [
                "query.filters[0].key",
                "query.filters[1].value",
                "query.filters[2].value",
                "query.filters[3].value",
                "query.filters[4].value",
                "query.filters[5].value",
                "query.filters[6].value",
            ]
        );
        let many: Vec<Value> =
            (0..=MAX_FILTERS).map(|_| json!({"key": "k", "op": "is_set"})).collect();
        assert_eq!(
            errors_of(json!({"kind": "events", "event": "e", "filters": many})),
            ["query.filters"]
        );
    }

    #[test]
    fn filters_encode_as_stored_text_and_numbers() {
        let filters: Vec<InsightFilter> = serde_json::from_value(json!([
            {"key": "n", "op": "is", "value": 2.0},
            {"key": "r", "op": "is", "value": 2.5},
            {"key": "t", "op": "gt", "value": "10"},
            {"key": "s", "op": "is_set"},
            {"key": "b", "op": "is", "value": true},
            {"key": "x", "op": "is", "value": "abc"},
        ]))
        .unwrap();
        let encoded: Value = serde_json::from_str(&encode_filters(&filters)).unwrap();
        assert_eq!(
            encoded,
            json!([
                {"k": "n", "op": "is", "v": "2", "n": 2.0},
                {"k": "r", "op": "is", "v": "2.5", "n": 2.5},
                {"k": "t", "op": "gt", "v": "10", "n": 10.0},
                {"k": "s", "op": "is_set", "v": null, "n": null},
                {"k": "b", "op": "is", "v": "true", "n": null},
                {"k": "x", "op": "is", "v": "abc", "n": null},
            ])
        );
    }

    #[test]
    fn unknown_fields_are_refused() {
        let query = json!({"kind": "events", "event": "e", "sql": "SELECT 1"});
        assert!(serde_json::from_value::<InsightQuery>(query).is_err());
    }
}
