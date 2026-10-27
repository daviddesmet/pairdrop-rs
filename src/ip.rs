//! Client IP resolution, ported from `_setIP` / `ipIsPrivate` in `server/peer.js`.
//!
//! PairDrop groups peers into rooms by IP, and deliberately collapses every private address to
//! `127.0.0.1` so that everyone on the server's own network lands in the same room.

use std::net::IpAddr;

use axum::http::HeaderMap;

use crate::config::Config;

/// Upstream reads `cf-connecting-ip` unconditionally, so anyone can forge it and join an
/// arbitrary IP room (schlagmichdoch/PairDrop#510). Only honour it when explicitly opted in.
pub fn trusts_cf_connecting_ip() -> bool {
    std::env::var("TRUST_CF_CONNECTING_IP").as_deref() == Ok("true")
}

pub fn resolve(headers: &HeaderMap, socket_addr: Option<IpAddr>, conf: &Config) -> String {
    let forwarded = if trusts_cf_connecting_ip() {
        first_entry(headers, "cf-connecting-ip").or_else(|| first_entry(headers, "x-forwarded-for"))
    } else {
        first_entry(headers, "x-forwarded-for")
    };

    let mut ip = match forwarded {
        Some(ip) => ip,
        None => socket_addr.map(|a| a.to_string()).unwrap_or_default(),
    };

    // IPv4-mapped IPv6 addresses carry the v4 address in the tail.
    if let Some(rest) = ip.strip_prefix("::ffff:") {
        ip = rest.to_string();
    }

    if let Some(segments) = conf.ipv6_localize {
        if ip.contains(':') {
            ip = ip.split(':').take(segments).collect::<Vec<_>>().join(":");
        }
    }

    if ip == "::1" || is_private(&ip) {
        return "127.0.0.1".to_string();
    }

    ip
}

fn first_entry(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(name)?.to_str().ok()?;
    let first = raw.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}

fn is_private(ip: &str) -> bool {
    if !ip.contains(':') {
        let octets: Vec<&str> = ip.split('.').collect();
        if octets.len() != 4 {
            return false;
        }
        return match (octets[0], octets[1].parse::<u8>()) {
            ("10", _) => true,
            ("172", Ok(second)) => (16..=31).contains(&second),
            ("192", _) => octets[1] == "168",
            _ => false,
        };
    }

    // First non-empty hextet decides; `::`-compressed addresses may start with empty ones.
    let Some(first) = ip.split(':').find(|segment| !segment.is_empty()) else {
        return false;
    };
    let first = first.to_ascii_lowercase();

    // Deprecated site-local (fec0::/10), unique local (fc00::/7), link local (fe80::/10),
    // and the discard prefix.
    let hex = |s: &str| s.chars().all(|c| c.is_ascii_hexdigit());
    match first.as_str() {
        "fe80" | "100" => true,
        s if s.len() == 4 && hex(s) => {
            let prefix = &s[..2];
            let third = s.as_bytes()[2] as char;
            (prefix == "fe" && ('c'..='f').contains(&third)) || prefix == "fc" || prefix == "fd"
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_private;

    #[test]
    fn ipv4_private_ranges() {
        assert!(is_private("10.0.0.1"));
        assert!(is_private("192.168.1.83"));
        assert!(is_private("172.16.0.1"));
        assert!(is_private("172.31.255.255"));
        assert!(!is_private("172.15.0.1"));
        assert!(!is_private("172.32.0.1"));
        assert!(!is_private("8.8.8.8"));
    }

    #[test]
    fn ipv6_local_ranges() {
        assert!(is_private("fe80::1"));
        assert!(is_private("fd00::1"));
        assert!(is_private("fc00::1"));
        assert!(is_private("fec0::1"));
        assert!(is_private("100::1"));
        assert!(!is_private("2001:db8::1"));
    }
}
