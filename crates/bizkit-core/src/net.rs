//! Egress rules for requests to user-supplied URLs: only public addresses.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use url::Url;

/// Why a URL may not be fetched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EgressError {
    /// Only `http` and `https` are allowed.
    #[error("only http and https URLs can be fetched, not {0}")]
    Scheme(String),
    /// The URL has no host.
    #[error("the URL has no host")]
    NoHost,
    /// The host is, or resolves only to, a non-public address.
    #[error("{host} is not a public address")]
    Blocked {
        /// The host as written in the URL.
        host: String,
    },
}

/// Whether `ip` is a public unicast address.
///
/// False for loopback, private, shared (CGNAT), link-local, multicast,
/// reserved, documentation and benchmarking ranges, and for IPv6 forms that
/// embed a non-public IPv4 address (mapped, compatible, NAT64, 6to4, Teredo).
#[must_use]
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let blocked = a == 0                                   // 0.0.0.0/8, "this network"; reaches loopback on Linux
        || a == 10                                          // 10/8
        || (a == 100 && (64..128).contains(&b))             // 100.64/10, shared address space
        || a == 127                                         // loopback
        || (a == 169 && b == 254)                           // link-local
        || (a == 172 && (16..32).contains(&b))              // 172.16/12
        || (a == 192 && b == 0 && c == 0)                   // 192.0.0/24, IETF protocol assignments
        || (a == 192 && b == 0 && c == 2)                   // TEST-NET-1
        || (a == 192 && b == 168)                           // 192.168/16
        || (a == 198 && (b == 18 || b == 19))               // 198.18/15, benchmarking
        || (a == 198 && b == 51 && c == 100)                // TEST-NET-2
        || (a == 203 && b == 0 && c == 113)                 // TEST-NET-3
        || a >= 224; // multicast 224/4, reserved 240/4 and broadcast
    !blocked
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = embedded_v4(ip) {
        return is_public_v4(v4);
    }
    let segments = ip.segments();
    let first = segments[0];
    let blocked = ip.is_unspecified()
        || ip.is_loopback()
        || (first == 0x2001 && segments[1] == 0x0db8) // documentation 2001:db8::/32
        || (first & 0xfe00) == 0xfc00                 // unique local fc00::/7
        || (first & 0xffc0) == 0xfe80                 // link-local fe80::/10
        || (first & 0xffc0) == 0xfec0                 // site-local fec0::/10
        || (first & 0xff00) == 0xff00; // multicast ff00::/8
    !blocked
}

/// The IPv4 address an IPv6 address carries, for the forms that route to it.
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let tail = |hi: u16, lo: u16| Ipv4Addr::from((u32::from(hi) << 16) | u32::from(lo));
    match ip.segments() {
        // ::ffff:a.b.c.d (mapped) and 64:ff9b::/96 (NAT64)
        [0, 0, 0, 0, 0, 0xffff, hi, lo] | [0x0064, 0xff9b, 0, 0, 0, 0, hi, lo] => {
            Some(tail(hi, lo))
        }
        // ::a.b.c.d (compatible, deprecated); :: and ::1 are handled as IPv6.
        [0, 0, 0, 0, 0, 0, hi, lo] if (hi, lo) > (0, 1) => Some(tail(hi, lo)),
        // 64:ff9b:1::/48 (local-use NAT64): treat the whole range as blocked.
        [0x0064, 0xff9b, 0x0001, ..] => Some(Ipv4Addr::LOCALHOST),
        // 2002::/16 (6to4): the IPv4 address sits in segments 1 and 2.
        [0x2002, hi, lo, ..] => Some(tail(hi, lo)),
        // 2001::/32 (Teredo): the client address is the last 32 bits, inverted.
        [0x2001, 0, .., hi, lo] => Some(tail(!hi, !lo)),
        _ => None,
    }
}

/// Checks a URL before any DNS lookup: the scheme, and IP-literal hosts,
/// which skip the resolver.
///
/// # Errors
///
/// When the scheme isn't http or https, there is no host, or the host is a
/// non-public IP literal.
pub fn check_url(url: &Url) -> Result<(), EgressError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(EgressError::Scheme(url.scheme().to_owned()));
    }
    let blocked = |ip: IpAddr| EgressError::Blocked { host: ip.to_string() };
    match url.host() {
        None => Err(EgressError::NoHost),
        Some(url::Host::Ipv4(ip)) if !is_public_v4(ip) => Err(blocked(ip.into())),
        Some(url::Host::Ipv6(ip)) if !is_public_v6(ip) => Err(blocked(ip.into())),
        Some(_) => Ok(()),
    }
}

/// A reqwest resolver that keeps only public addresses. The client connects
/// to what this returns, so a name that resolves differently on a second
/// lookup (DNS rebinding) can't reach an internal address.
#[derive(Debug, Clone, Copy, Default)]
pub struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|addr| is_public_ip(addr.ip()))
                .collect();
            if addrs.is_empty() {
                let err: Box<dyn std::error::Error + Send + Sync> =
                    Box::new(EgressError::Blocked { host });
                return Err(err);
            }
            let addrs: reqwest::dns::Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(ip: &str) -> bool {
        is_public_ip(ip.parse().unwrap())
    }

    #[test]
    fn blocks_every_non_public_range() {
        let blocked = [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "127.255.255.255",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::127.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b::a00:1",
            "64:ff9b:1::1",
            "2002:7f00:1::",
            "2002:c0a8:101::",
            "2001:0:4136:e378:8000:63bf:80ff:fffe",
            "2001:db8::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
        ];
        for ip in blocked {
            assert!(!public(ip), "{ip} must be blocked");
        }
    }

    #[test]
    fn allows_public_addresses_next_to_blocked_ranges() {
        let allowed = [
            "1.1.1.1",
            "8.8.8.8",
            "9.255.255.255",
            "11.0.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "126.255.255.255",
            "128.0.0.1",
            "169.253.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.0.1.1",
            "192.167.1.1",
            "192.169.0.1",
            "198.17.255.255",
            "198.20.0.1",
            "223.255.255.255",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:808:808::",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ];
        for ip in allowed {
            assert!(public(ip), "{ip} must be allowed");
        }
    }

    #[test]
    fn checks_scheme_and_ip_literal_hosts() {
        let check = |url: &str| check_url(&Url::parse(url).unwrap());
        assert_eq!(check("https://example.com/x"), Ok(()), "public name");
        assert_eq!(check("http://8.8.8.8/"), Ok(()), "public IP");
        assert!(matches!(check("file:///etc/passwd"), Err(EgressError::Scheme(_))), "file");
        assert!(matches!(check("ftp://example.com/"), Err(EgressError::Scheme(_))), "ftp");
        assert!(matches!(check("http://127.0.0.1:8080/"), Err(EgressError::Blocked { .. })));
        assert!(matches!(check("http://[::1]/"), Err(EgressError::Blocked { .. })), "v6 loopback");
        assert!(matches!(check("http://0x7f000001/"), Err(EgressError::Blocked { .. })), "hex IP");
        assert!(matches!(check("http://2130706433/"), Err(EgressError::Blocked { .. })), "int IP");
    }
}
