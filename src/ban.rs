use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

pub struct BanList {
    threshold: usize,
    counts: HashMap<IpAddr, usize>,
    banned: HashSet<IpAddr>,
}

impl BanList {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            counts: HashMap::new(),
            banned: HashSet::new(),
        }
    }

    pub fn record(&mut self, ip: IpAddr) -> bool {
        let count = self.counts.entry(ip).or_insert(0);
        *count += 1;

        if *count >= self.threshold {
            self.banned.insert(ip);
        }

        self.is_banned(&ip)
    }

    pub fn is_banned(&self, ip: &IpAddr) -> bool {
        self.banned.contains(ip)
    }

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
