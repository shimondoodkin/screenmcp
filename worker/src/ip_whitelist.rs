//! Shared IP whitelist parsing and CIDR matching.
//! Used by both the open-source (file-based) and cloud (API-based) workers.

use std::net::IpAddr;

use tracing::warn;

/// IP whitelist entry — either a single IP or a CIDR range.
#[derive(Debug, Clone)]
enum IpEntry {
    Single(IpAddr),
    Cidr(IpAddr, u8),
}

impl IpEntry {
    fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if let Some((addr_str, prefix_str)) = s.split_once('/') {
            let addr: IpAddr = addr_str
                .parse()
                .map_err(|e| format!("invalid IP in CIDR '{s}': {e}"))?;
            let prefix: u8 = prefix_str
                .parse()
                .map_err(|e| format!("invalid prefix in CIDR '{s}': {e}"))?;
            let max = if addr.is_ipv4() { 32 } else { 128 };
            if prefix > max {
                return Err(format!("prefix /{prefix} too large for {addr}"));
            }
            Ok(IpEntry::Cidr(addr, prefix))
        } else {
            let addr: IpAddr = s
                .parse()
                .map_err(|e| format!("invalid IP '{s}': {e}"))?;
            Ok(IpEntry::Single(addr))
        }
    }

    fn contains(&self, ip: &IpAddr) -> bool {
        match self {
            IpEntry::Single(addr) => addr == ip,
            IpEntry::Cidr(network, prefix) => match (network, ip) {
                (IpAddr::V4(net), IpAddr::V4(check)) => {
                    if *prefix == 0 {
                        return true;
                    }
                    let net_bits = u32::from(*net);
                    let check_bits = u32::from(*check);
                    let mask = u32::MAX << (32 - prefix);
                    (net_bits & mask) == (check_bits & mask)
                }
                (IpAddr::V6(net), IpAddr::V6(check)) => {
                    if *prefix == 0 {
                        return true;
                    }
                    let net_bits = u128::from(*net);
                    let check_bits = u128::from(*check);
                    let mask = u128::MAX << (128 - prefix);
                    (net_bits & mask) == (check_bits & mask)
                }
                _ => false, // v4 vs v6 mismatch
            },
        }
    }
}

/// Parsed IP whitelist. Shared between file-based and API-based backends.
/// Empty = whitelist disabled (all IPs allowed).
#[derive(Debug, Clone)]
pub struct IpWhitelist {
    entries: Vec<IpEntry>,
}

impl IpWhitelist {
    /// Parse from a list of IP/CIDR strings (e.g. from TOML array).
    /// Invalid entries are logged and skipped.
    pub fn from_list(items: &[String]) -> Self {
        let entries = items
            .iter()
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() {
                    return None;
                }
                match IpEntry::parse(s) {
                    Ok(entry) => Some(entry),
                    Err(e) => {
                        warn!("skipping invalid ip_whitelist entry: {e}");
                        None
                    }
                }
            })
            .collect();
        Self { entries }
    }

    /// Parse from a newline-separated text blob (e.g. from database text field).
    /// Invalid entries are logged and skipped.
    pub fn from_text(text: &str) -> Self {
        let items: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        Self::from_list(&items)
    }

    /// Returns true if the whitelist is empty (disabled).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check if an IP address string is allowed.
    /// Returns Ok(()) if allowed, Err with message if blocked.
    /// Empty whitelist always returns Ok (disabled).
    pub fn check(&self, ip_address: &str) -> Result<(), String> {
        if self.entries.is_empty() {
            return Ok(());
        }

        let ip: IpAddr = ip_address
            .parse()
            .map_err(|e| format!("invalid client IP '{ip_address}': {e}"))?;

        if self.entries.iter().any(|entry| entry.contains(&ip)) {
            Ok(())
        } else {
            Err("IP not in whitelist".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_ip_match() {
        let wl = IpWhitelist::from_list(&["192.168.1.1".into()]);
        assert!(wl.check("192.168.1.1").is_ok());
        assert!(wl.check("192.168.1.2").is_err());
    }

    #[test]
    fn test_cidr_match() {
        let wl = IpWhitelist::from_list(&["10.0.0.0/8".into()]);
        assert!(wl.check("10.1.2.3").is_ok());
        assert!(wl.check("10.255.255.255").is_ok());
        assert!(wl.check("11.0.0.1").is_err());
    }

    #[test]
    fn test_cidr_24() {
        let wl = IpWhitelist::from_list(&["192.168.1.0/24".into()]);
        assert!(wl.check("192.168.1.0").is_ok());
        assert!(wl.check("192.168.1.255").is_ok());
        assert!(wl.check("192.168.2.0").is_err());
    }

    #[test]
    fn test_from_text() {
        let wl = IpWhitelist::from_text("10.0.0.0/8\n192.168.1.1\n\n  203.0.113.0/24  ");
        assert!(wl.check("10.5.5.5").is_ok());
        assert!(wl.check("192.168.1.1").is_ok());
        assert!(wl.check("203.0.113.50").is_ok());
        assert!(wl.check("8.8.8.8").is_err());
    }

    #[test]
    fn test_empty_allows_all() {
        let wl = IpWhitelist::from_list(&[]);
        assert!(wl.is_empty());
        assert!(wl.check("1.2.3.4").is_ok());
    }

    #[test]
    fn test_empty_text_allows_all() {
        let wl = IpWhitelist::from_text("");
        assert!(wl.is_empty());
        assert!(wl.check("1.2.3.4").is_ok());
    }
}
