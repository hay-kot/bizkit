//! Rules for names users and sites send: event names, goal paths and
//! hostname patterns. Shared by the ops and the ingest parser.

/// The longest event name.
pub const MAX_EVENT_NAME: usize = 64;
/// The longest page path stored or matched.
pub const MAX_PATH: usize = 512;
/// What [`is_event_name`] accepts, for error messages.
pub const EVENT_NAME_RULE: &str = "must be 1-64 letters, digits, or _ . : -";

/// Whether `name` is a custom event name: 1-64 of `[A-Za-z0-9_.:-]`.
#[must_use]
pub fn is_event_name(name: &str) -> bool {
    (1..=MAX_EVENT_NAME).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
}

/// Whether `path` can be a page goal: starts with `/`, no wildcard, query
/// or fragment, at most [`MAX_PATH`] bytes.
#[must_use]
pub fn is_goal_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= MAX_PATH
        && !path.contains(['*', '?', '#'])
        && !path.chars().any(char::is_whitespace)
}

/// Whether `pattern` is a hostname (`example.com`) or a suffix pattern
/// (`*.example.com`). Lowercase is not required; callers normalize.
#[must_use]
pub fn is_hostname_pattern(pattern: &str) -> bool {
    let host = pattern.strip_prefix("*.").unwrap_or(pattern);
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Whether `host` is allowed by `patterns`. An empty list allows any host.
/// `*.example.com` allows subdomains of example.com, not example.com itself.
#[must_use]
pub fn host_allowed(patterns: &[String], host: &str) -> bool {
    patterns.is_empty()
        || patterns.iter().any(|pattern| {
            pattern.strip_prefix("*.").map_or_else(
                || pattern.eq_ignore_ascii_case(host),
                |suffix| {
                    host.strip_suffix(suffix)
                        .and_then(|rest| rest.strip_suffix('.'))
                        .is_some_and(|sub| !sub.is_empty())
                },
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names() {
        for good in ["signup", "signup_completed", "checkout:start", "v1.2-beta"] {
            assert!(is_event_name(good), "{good}");
        }
        for bad in ["", "has space", "emoji🙂", "slash/", &"x".repeat(65)] {
            assert!(!is_event_name(bad), "{bad}");
        }
    }

    #[test]
    fn goal_paths() {
        assert!(is_goal_path("/"));
        assert!(is_goal_path("/signup/done"));
        for bad in ["signup", "/blog/*", "/a?b=1", "/a#x", "/a b"] {
            assert!(!is_goal_path(bad), "{bad}");
        }
    }

    #[test]
    fn hostname_patterns() {
        for good in ["example.com", "*.example.com", "localhost", "a-b.example.co.uk"] {
            assert!(is_hostname_pattern(good), "{good}");
        }
        for bad in ["", "*.", "https://example.com", "example.com:8080", "-a.com", "a..com", "*"] {
            assert!(!is_hostname_pattern(bad), "{bad}");
        }
    }

    #[test]
    fn hosts_match_exact_or_subdomain() {
        let patterns = vec!["example.com".to_owned(), "*.app.io".to_owned()];
        assert!(host_allowed(&patterns, "example.com"));
        assert!(host_allowed(&patterns, "EXAMPLE.com"));
        assert!(!host_allowed(&patterns, "www.example.com"));
        assert!(host_allowed(&patterns, "eu.app.io"));
        assert!(!host_allowed(&patterns, "app.io"));
        assert!(!host_allowed(&patterns, "evilapp.io"));
        assert!(host_allowed(&[], "anything.test"));
    }
}
