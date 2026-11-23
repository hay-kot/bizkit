//! What an event's URL, referrer and sender add to it: where the visit
//! came from and the daily visitor hash.

use std::net::IpAddr;

use bizkit_core::ids::ProjectId;
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::names::MAX_PATH;

/// The longest UTM value kept, in characters.
pub const MAX_UTM: usize = 128;

/// Where a visit came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribution {
    /// The linking site's host, without `www.`; none for self-referrals.
    pub referrer_host: Option<String>,
    /// `utm_source`, or `ref` when there is no `utm_source`.
    pub utm_source: Option<String>,
    /// `utm_medium`.
    pub utm_medium: Option<String>,
    /// `utm_campaign`.
    pub utm_campaign: Option<String>,
}

fn bare_host(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(host.strip_prefix("www.").map(str::to_owned).unwrap_or(host))
}

fn utm(url: &Url, key: &str) -> Option<String> {
    url.query_pairs().find(|(k, _)| k == key).and_then(|(_, value)| {
        let value: String = value.trim().chars().take(MAX_UTM).collect();
        (!value.is_empty()).then_some(value)
    })
}

/// The traffic source of a page `url` reached from `referrer`.
#[must_use]
pub fn source(url: &Url, referrer: Option<&Url>) -> Attribution {
    let own = bare_host(url);
    let referrer_host = referrer.and_then(bare_host).filter(|host| Some(host) != own.as_ref());
    Attribution {
        referrer_host,
        utm_source: utm(url, "utm_source").or_else(|| utm(url, "ref")),
        utm_medium: utm(url, "utm_medium"),
        utm_campaign: utm(url, "utm_campaign"),
    }
}

/// The page path reports show: the URL path, at most [`MAX_PATH`] bytes.
#[must_use]
pub fn path(url: &Url) -> String {
    let path = url.path();
    let mut end = path.len().min(MAX_PATH);
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    path.get(..end).unwrap_or("/").to_owned()
}

/// Who sent an event, for the visitor hash.
#[derive(Debug, Clone, Copy)]
pub enum Who<'a> {
    /// A browser: its address (when known) and User-Agent.
    Web {
        /// The client address.
        ip: Option<IpAddr>,
        /// The User-Agent header.
        ua: &'a str,
    },
    /// A server event for a known user.
    User(&'a str),
    /// A server event for an anonymous ID.
    Anon(&'a str),
}

/// The daily visitor hash.
///
/// SHA-256 over the day's salt, the project and the sender, cut to 64 bits.
/// The salt rotates daily and is deleted, so a hash can't be traced back to
/// an address or linked across days.
#[must_use]
pub fn visitor(salt: &[u8; 32], project: ProjectId, who: Who<'_>) -> i64 {
    let mut hash = Sha256::new();
    hash.update(salt);
    hash.update(project.to_string().as_bytes());
    match who {
        Who::Web { ip, ua } => {
            hash.update(b"\0web\0");
            if let Some(ip) = ip {
                hash.update(ip.to_string().as_bytes());
            }
            hash.update(b"\0");
            hash.update(ua.as_bytes());
        }
        Who::User(id) => {
            hash.update(b"\0user\0");
            hash.update(id.as_bytes());
        }
        Who::Anon(id) => {
            hash.update(b"\0anon\0");
            hash.update(id.as_bytes());
        }
    }
    let digest = hash.finalize();
    let mut first = [0_u8; 8];
    first.copy_from_slice(digest.get(..8).unwrap_or(&[0; 8]));
    i64::from_be_bytes(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn sources() {
        let page = url(
            "https://www.example.com/p?utm_source=hn&utm_medium=social&utm_campaign=%20launch%20",
        );
        let from_hn = source(&page, Some(&url("https://news.ycombinator.com/item?id=1")));
        assert_eq!(
            from_hn,
            Attribution {
                referrer_host: Some("news.ycombinator.com".to_owned()),
                utm_source: Some("hn".to_owned()),
                utm_medium: Some("social".to_owned()),
                utm_campaign: Some("launch".to_owned()),
            }
        );
        let internal = source(&page, Some(&url("https://example.com/other")));
        assert_eq!(internal.referrer_host, None, "self-referral");
        let by_ref = source(&url("https://x.test/?ref=producthunt"), None);
        assert_eq!(by_ref.utm_source.as_deref(), Some("producthunt"));
        assert_eq!(source(&url("https://x.test/?utm_source="), None).utm_source, None);
    }

    #[test]
    fn paths_are_capped() {
        assert_eq!(path(&url("https://x.test/a/b?q=1#f")), "/a/b");
        let long = format!("https://x.test/{}", "a".repeat(600));
        assert_eq!(path(&url(&long)).len(), MAX_PATH);
    }

    #[test]
    fn visitors_differ_by_salt_project_and_sender() {
        let project = ProjectId::new();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let web = Who::Web { ip: Some(ip), ua: "UA" };
        let a = visitor(&[1; 32], project, web);
        assert_eq!(a, visitor(&[1; 32], project, web), "stable within a day");
        assert_ne!(a, visitor(&[2; 32], project, web), "salt");
        assert_ne!(a, visitor(&[1; 32], ProjectId::new(), web), "project");
        assert_ne!(a, visitor(&[1; 32], project, Who::Web { ip: Some(ip), ua: "UA2" }), "ua");
        assert_ne!(
            visitor(&[1; 32], project, Who::User("x")),
            visitor(&[1; 32], project, Who::Anon("x"))
        );
    }

    #[test]
    fn fuzz_source() {
        bolero::check!().with_type::<(String, String)>().for_each(|(page, referrer)| {
            let Ok(page) = Url::parse(page) else { return };
            let referrer = Url::parse(referrer).ok();
            let found = source(&page, referrer.as_ref());
            for value in
                [&found.utm_source, &found.utm_medium, &found.utm_campaign].into_iter().flatten()
            {
                assert!(!value.is_empty() && value.chars().count() <= MAX_UTM, "utm bounds");
            }
            assert!(path(&page).len() <= MAX_PATH, "path bound");
        });
    }
}
