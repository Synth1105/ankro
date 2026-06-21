//! IP-based request banning for the `ankro` bridge.

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

/// Tracks request counts per IP and marks callers as banned once they cross a threshold.
pub struct BanList {
    threshold: usize,
    counts: HashMap<IpAddr, usize>,
    banned: HashSet<IpAddr>,
}

impl BanList {
    /// Create a new ban list with the provided threshold.
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            counts: HashMap::new(),
            banned: HashSet::new(),
        }
    }

    /// Record one request from `ip` and return whether that IP is now banned.
    pub fn record(&mut self, ip: IpAddr) -> bool {
        tracing::debug!("recording ip {ip}");
        let count = self.counts.entry(ip).or_insert(0);
        *count += 1;

        if *count >= self.threshold {
            self.banned.insert(ip);
        }
        
        self.is_banned(&ip)
    }

    /// Check whether `ip` is currently banned.
    pub fn is_banned(&self, ip: &IpAddr) -> bool {
        if  self.banned.contains(ip) {
            tracing::debug!("{ip} is banned");
        };
        self.banned.contains(ip)
    }

    /// Return the current request counter map.
    pub fn counts(&self) -> &HashMap<IpAddr, usize> {
        &self.counts
    }
}

#[cfg(test)]
mod tests {
    use super::BanList;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn bans_after_threshold() {
        let mut bans = BanList::new(3);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        assert!(!bans.record(ip));
        assert!(!bans.record(ip));
        assert!(bans.record(ip));
        assert!(bans.is_banned(&ip));
    }
}
