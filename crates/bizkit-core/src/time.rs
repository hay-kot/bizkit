//! Timestamps. The database stores unix milliseconds so comparisons and
//! indexes stay exact; the API speaks RFC 3339.

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use utoipa::openapi::schema::{KnownFormat, ObjectBuilder, SchemaFormat, Type};
use utoipa::openapi::{RefOr, Schema};

/// A point in time as milliseconds since the unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct UnixMillis(pub i64);

impl UnixMillis {
    /// This instant plus `duration`, saturating at the end of time.
    #[must_use]
    pub fn plus(self, duration: Duration) -> Self {
        let millis = i64::try_from(duration.as_millis()).unwrap_or(i64::MAX);
        Self(self.0.saturating_add(millis))
    }

    /// This instant minus `duration`, saturating at the start of time.
    #[must_use]
    pub fn minus(self, duration: Duration) -> Self {
        let millis = i64::try_from(duration.as_millis()).unwrap_or(i64::MAX);
        Self(self.0.saturating_sub(millis))
    }

    /// The instant as a UTC date-time, or `None` past the range `time` supports.
    #[must_use]
    pub fn to_datetime(self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(self.0) * 1_000_000).ok()
    }

    /// The RFC 3339 form, as the API serializes it.
    ///
    /// # Errors
    ///
    /// When formatting fails, which needs a date outside year 0-9999.
    pub fn to_rfc3339(self) -> Result<String, time::error::Format> {
        let datetime = self.to_datetime().unwrap_or(OffsetDateTime::UNIX_EPOCH);
        datetime.format(&Rfc3339)
    }
}

impl fmt::Display for UnixMillis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_rfc3339() {
            Ok(text) => f.write_str(&text),
            Err(_) => write!(f, "{}ms", self.0),
        }
    }
}

impl Serialize for UnixMillis {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.to_rfc3339().map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&text)
    }
}

impl<'de> Deserialize<'de> for UnixMillis {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Cow::<'de, str>::deserialize(deserializer)?;
        let datetime = OffsetDateTime::parse(&text, &Rfc3339).map_err(serde::de::Error::custom)?;
        let millis = datetime.unix_timestamp_nanos() / 1_000_000;
        i64::try_from(millis).map(Self).map_err(serde::de::Error::custom)
    }
}

impl utoipa::PartialSchema for UnixMillis {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::DateTime)))
            .into()
    }
}

impl utoipa::ToSchema for UnixMillis {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("Timestamp")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_rfc3339() {
        let json = serde_json::to_string(&UnixMillis(1_700_000_000_123)).unwrap();
        assert_eq!(json, "\"2023-11-14T22:13:20.123Z\"", "timestamps are RFC 3339 in UTC");
    }

    #[test]
    fn round_trips_through_json() {
        let at = UnixMillis(1_700_000_000_123);
        let back: UnixMillis = serde_json::from_str(&serde_json::to_string(&at).unwrap()).unwrap();
        assert_eq!(back, at, "a timestamp survives a JSON round trip");
    }

    #[test]
    fn add_saturates() {
        assert_eq!(UnixMillis(i64::MAX).plus(Duration::from_secs(1)), UnixMillis(i64::MAX), "add");
    }
}
