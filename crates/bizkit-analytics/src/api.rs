//! The analytics module's API types.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{Date, Month, OffsetDateTime, Time};
use utoipa::ToSchema;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{KnownFormat, ObjectBuilder, Schema, SchemaFormat, Type};

bizkit_core::id_type!(GoalId, "The ID of an analytics goal.");
bizkit_core::id_type!(FunnelId, "The ID of an analytics funnel.");
bizkit_core::id_type!(PersonId, "The ID of an identified person in analytics.");
bizkit_core::id_type!(InsightId, "The ID of a saved analytics insight.");
bizkit_core::id_type!(DashboardId, "The ID of an analytics dashboard.");

/// A UTC calendar day, written `YYYY-MM-DD`. Analytics counts by UTC day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(Date);

impl Day {
    /// The UTC day `at` falls on.
    #[must_use]
    pub fn of(at: UnixMillis) -> Self {
        let nanos = i128::from(at.0) * 1_000_000;
        let date = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .unwrap_or(OffsetDateTime::UNIX_EPOCH)
            .date();
        Self(date)
    }

    /// Midnight UTC at the start of the day.
    #[must_use]
    pub const fn start(self) -> UnixMillis {
        let at = self.0.with_time(Time::MIDNIGHT).assume_utc();
        UnixMillis(at.unix_timestamp().saturating_mul(1000))
    }

    /// The day after, or this day at the end of the calendar.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.next_day().unwrap_or(self.0))
    }

    /// The day before, or this day at the start of the calendar.
    #[must_use]
    pub fn previous(self) -> Self {
        Self(self.0.previous_day().unwrap_or(self.0))
    }

    /// Whole days from `self` to `later`; negative when `later` is earlier.
    #[must_use]
    pub fn days_until(self, later: Self) -> i64 {
        (later.0 - self.0).whole_days()
    }

    /// `n` days later (or earlier, for a negative `n`), clamped to the calendar.
    #[must_use]
    pub fn plus_days(self, n: i64) -> Self {
        Self(self.0.checked_add(time::Duration::days(n)).unwrap_or(self.0))
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.0.year(), u8::from(self.0.month()), self.0.day())
    }
}

/// Not a `YYYY-MM-DD` calendar day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadDay;

impl fmt::Display for BadDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("must be a date written YYYY-MM-DD")
    }
}

impl std::error::Error for BadDay {}

impl FromStr for Day {
    type Err = BadDay;

    fn from_str(text: &str) -> Result<Self, BadDay> {
        let mut parts = text.split('-');
        let (Some(year), Some(month), Some(day), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(BadDay);
        };
        if year.len() != 4 || month.len() != 2 || day.len() != 2 {
            return Err(BadDay);
        }
        let year: i32 = year.parse().map_err(|_| BadDay)?;
        let month: u8 = month.parse().map_err(|_| BadDay)?;
        let day: u8 = day.parse().map_err(|_| BadDay)?;
        let month = Month::try_from(month).map_err(|_| BadDay)?;
        Date::from_calendar_date(year, month, day).map(Self).map_err(|_| BadDay)
    }
}

impl Serialize for Day {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Day {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Cow::<str>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl utoipa::PartialSchema for Day {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Date)))
            .into()
    }
}

impl ToSchema for Day {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("Day")
    }
}

/// What the snippet may store in a visitor's browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityMode {
    /// Nothing. Visitors are a daily hash, and `identify` is ignored.
    #[default]
    Cookieless,
    /// An anonymous ID, only once the site calls `identify`.
    AfterIdentify,
    /// An anonymous ID from the first pageview. For sites that collect consent.
    Always,
}

impl IdentityMode {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cookieless => "cookieless",
            Self::AfterIdentify => "after_identify",
            Self::Always => "always",
        }
    }

    /// Reads the stored text; unknown text is the safest mode.
    #[must_use]
    pub fn from_stored(text: &str) -> Self {
        match text {
            "after_identify" => Self::AfterIdentify,
            "always" => Self::Always,
            _ => Self::Cookieless,
        }
    }
}

/// A project's analytics settings and snippet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Site {
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The public key in the snippet tag.
    pub key: String,
    /// The `<script>` tag to add to every page.
    pub snippet: String,
    /// Hostnames the snippet may send from, exact or `*.example.com`. Empty
    /// means any host.
    pub hostnames: Vec<String>,
    /// What the snippet may store in a visitor's browser.
    pub identity_mode: IdentityMode,
    /// Whether any event has arrived yet.
    pub has_events: bool,
    /// When analytics was turned on.
    pub created_at: UnixMillis,
    /// When its settings last changed.
    pub updated_at: UnixMillis,
}

/// What a goal matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    /// A pageview of exactly this path.
    Page,
    /// A custom event with exactly this name.
    Event,
}

impl GoalKind {
    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Event => "event",
        }
    }

    /// Reads the stored text.
    #[must_use]
    pub fn from_stored(text: &str) -> Self {
        if text == "event" { Self::Event } else { Self::Page }
    }
}

/// A page or event that counts as a conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Goal {
    /// The goal ID.
    pub id: GoalId,
    /// The project it belongs to.
    pub project_id: ProjectId,
    /// The display name.
    pub name: String,
    /// What it matches.
    pub kind: GoalKind,
    /// The page path (`/signup/done`) or event name (`signup_completed`).
    #[serde(rename = "match")]
    pub matches: String,
    /// When it was created.
    pub created_at: UnixMillis,
}

/// A project's goals.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GoalList {
    /// Sorted by name.
    pub items: Vec<Goal>,
}

/// What a breakdown groups by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    /// Page path. Pageviews only.
    Page,
    /// The linking site; empty for direct traffic. Pageviews only.
    Referrer,
    /// `utm_source` (or `ref`). Pageviews only.
    UtmSource,
    /// `utm_medium`. Pageviews only.
    UtmMedium,
    /// `utm_campaign`. Pageviews only.
    UtmCampaign,
    /// ISO country code; empty when unknown. Pageviews only.
    Country,
    /// `desktop`, `mobile` or `tablet`. Pageviews only.
    Device,
    /// Browser family. Pageviews only.
    Browser,
    /// OS family. Pageviews only.
    Os,
    /// Custom event name. Custom events only.
    Event,
    /// Each goal, with conversions and conversion rate.
    Goal,
}

impl Dimension {
    /// Every dimension the daily rollup stores. Goals read the page and
    /// event rows.
    pub const ROLLED: [Self; 10] = [
        Self::Page,
        Self::Referrer,
        Self::UtmSource,
        Self::UtmMedium,
        Self::UtmCampaign,
        Self::Country,
        Self::Device,
        Self::Browser,
        Self::Os,
        Self::Event,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Referrer => "referrer",
            Self::UtmSource => "utm_source",
            Self::UtmMedium => "utm_medium",
            Self::UtmCampaign => "utm_campaign",
            Self::Country => "country",
            Self::Device => "device",
            Self::Browser => "browser",
            Self::Os => "os",
            Self::Event => "event",
            Self::Goal => "goal",
        }
    }
}

/// Counts over a date range. Visitors are summed over days, because the
/// visitor hash changes every day.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Totals {
    /// Unique visitors, summed per day.
    pub visitors: i64,
    /// Pageviews.
    pub pageviews: i64,
    /// Custom events.
    pub events: i64,
    /// Goal conversions: unique visitors per day who reached a goal, over
    /// every goal.
    pub conversions: i64,
}

/// How long one chart point covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interval {
    /// One hour, for a one-day range that still has raw events.
    Hour,
    /// One UTC day.
    Day,
}

/// One chart point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SeriesPoint {
    /// When the hour or day starts.
    pub start: UnixMillis,
    /// Unique visitors.
    pub visitors: i64,
    /// Pageviews.
    pub pageviews: i64,
}

/// The Overview's tiles and chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Summary {
    /// Counts for the range.
    pub totals: Totals,
    /// Counts for the previous period of the same length, when asked.
    pub previous: Option<Totals>,
    /// What one chart point covers: hours for a one-day range, else days.
    /// A one-day range past raw retention has one daily point.
    pub interval: Interval,
    /// Visitors and pageviews over the range, one point per interval.
    pub series: Vec<SeriesPoint>,
    /// The same for the previous period, when asked.
    pub previous_series: Option<Vec<SeriesPoint>>,
}

/// One row of a breakdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BreakdownRow {
    /// The page, source, country, event name or goal name. Empty means
    /// direct traffic or unknown.
    pub value: String,
    /// Unique visitors, summed per day. For goals, the conversions.
    pub visitors: i64,
    /// Pageviews.
    pub pageviews: i64,
    /// Custom events.
    pub events: i64,
    /// The goal, for the goal dimension.
    pub goal_id: Option<GoalId>,
    /// Conversions over unique visitors in the range, 0 to 1, for the goal
    /// dimension.
    pub conversion_rate: Option<f64>,
}

/// A breakdown, most visitors first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Breakdown {
    /// The rows.
    pub items: Vec<BreakdownRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_utc_and_round_trip() {
        let day = Day::of(UnixMillis(1_700_000_000_123));
        assert_eq!(day.to_string(), "2023-11-14");
        assert_eq!(day.start(), UnixMillis(1_699_920_000_000));
        assert_eq!(Day::of(day.start()), day, "midnight is the same day");
        assert_eq!(Day::of(UnixMillis(day.start().0 - 1)), day.previous());
        assert_eq!("2023-11-14".parse::<Day>(), Ok(day));
        assert_eq!(day.next().to_string(), "2023-11-15");
        assert_eq!(day.days_until(day.plus_days(30)), 30);
        let json: Day = serde_json::from_str("\"2024-02-29\"").unwrap();
        assert_eq!(serde_json::to_string(&json).unwrap(), "\"2024-02-29\"");
    }

    #[test]
    fn bad_days() {
        for bad in ["", "2023-02-30", "2023-1-01", "23-01-01", "2023-01-01T00:00", "2023/01/01"] {
            assert_eq!(bad.parse::<Day>(), Err(BadDay), "{bad}");
        }
    }
}
