//! In-memory ingest rate limits over fixed one-minute windows. Counters
//! reset on restart, which is fine for abuse limits.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};

use bizkit_core::ids::ProjectId;
use bizkit_core::time::UnixMillis;

/// Events per site per minute.
pub const PER_SITE: u32 = 1_000;
/// Events per site and client address per minute.
pub const PER_CLIENT: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Site(ProjectId),
    /// IPv6 clients are keyed by their /64, which one host usually owns whole.
    Client(ProjectId, u128),
}

#[derive(Debug, Default)]
struct Window {
    minute: i64,
    counts: HashMap<Key, u32>,
}

/// The rate limiter.
#[derive(Debug, Default)]
pub struct Limiter {
    window: Mutex<Window>,
}

fn client_block(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(v4) => u128::from(u32::from(v4)),
        IpAddr::V6(v6) => u128::from(v6) & !u128::from(u64::MAX),
    }
}

impl Limiter {
    /// Counts one event for `project` from `ip` and says whether it is
    /// within the limits. Events from an unknown address count only toward
    /// the site limit.
    pub fn allow(&self, project: ProjectId, ip: Option<IpAddr>, now: UnixMillis) -> bool {
        let mut window = self.window.lock().unwrap_or_else(PoisonError::into_inner);
        let minute = now.0.div_euclid(60_000);
        if window.minute != minute {
            window.minute = minute;
            window.counts.clear();
        }
        let site = Key::Site(project);
        let client = ip.map(|ip| Key::Client(project, client_block(ip)));
        let count = |key: &Key| window.counts.get(key).copied().unwrap_or(0);
        if count(&site) >= PER_SITE || client.is_some_and(|key| count(&key) >= PER_CLIENT) {
            return false;
        }
        for key in std::iter::once(site).chain(client) {
            *window.counts.entry(key).or_insert(0) += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_client_then_window_reset() {
        let limiter = Limiter::default();
        let project = ProjectId::new();
        let ip: IpAddr = "203.0.113.1".parse().unwrap();
        let now = UnixMillis(1_700_000_040_000);
        for _ in 0..PER_CLIENT {
            assert!(limiter.allow(project, Some(ip), now));
        }
        assert!(!limiter.allow(project, Some(ip), now), "over the client limit");
        assert!(limiter.allow(project, Some("203.0.113.2".parse().unwrap()), now), "other client");
        assert!(limiter.allow(ProjectId::new(), Some(ip), now), "other site");
        assert!(limiter.allow(project, Some(ip), UnixMillis(now.0 + 60_000)), "next minute");
    }

    #[test]
    fn ipv6_clients_share_their_64() {
        let limiter = Limiter::default();
        let project = ProjectId::new();
        let now = UnixMillis(0);
        for i in 0..PER_CLIENT {
            let ip: IpAddr = format!("2001:db8::{i:x}").parse().unwrap();
            assert!(limiter.allow(project, Some(ip), now));
        }
        assert!(!limiter.allow(project, Some("2001:db8::ffff".parse().unwrap()), now));
        assert!(limiter.allow(project, Some("2001:db8:0:1::1".parse().unwrap()), now));
    }

    #[test]
    fn per_site_counts_unknown_addresses() {
        let limiter = Limiter::default();
        let project = ProjectId::new();
        for _ in 0..PER_SITE {
            assert!(limiter.allow(project, None, UnixMillis(0)));
        }
        assert!(!limiter.allow(project, None, UnixMillis(0)));
    }

    #[test]
    fn fuzz_allow() {
        let limiter = Limiter::default();
        let project = ProjectId::new();
        bolero::check!().with_type::<(u128, u32, bool, i64)>().for_each(|(v6, v4, is_v6, at)| {
            let ip = if *is_v6 {
                IpAddr::V6(std::net::Ipv6Addr::from(*v6))
            } else {
                IpAddr::V4(std::net::Ipv4Addr::from(*v4))
            };
            let _allowed = limiter.allow(project, Some(ip), UnixMillis(*at));
        });
    }
}
