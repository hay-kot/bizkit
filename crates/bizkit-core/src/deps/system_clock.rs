use std::time::{SystemTime, UNIX_EPOCH};

use super::Clock;
use crate::time::UnixMillis;

/// The wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixMillis {
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        UnixMillis(i64::try_from(millis).unwrap_or(i64::MAX))
    }
}
