//! Thread-safe pool of IPv4 `/24` subnets for concurrent game match allocation.
//!
//! Subnets are allocated from a configured parent network (e.g., `10.200.0.0/16`)
//! and returned when the match ends. Up to 256 concurrent matches are supported
//! with a `/16` pool.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Mutex;

use ipnet::Ipv4Net;

/// Thread-safe pool of `/24` subnets allocated from a parent network.
#[derive(Debug)]
pub struct SubnetPool {
    /// The parent network from which /24s are carved (e.g., 10.200.0.0/16)
    parent: Ipv4Net,
    /// Set of third-octet values currently in use (0–255)
    in_use: Mutex<HashSet<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum SubnetPoolError {
    #[error("No subnets available in pool {0}")]
    Exhausted(Ipv4Net),
}

impl SubnetPool {
    /// Create a new subnet pool from the given parent network.
    ///
    /// The parent should be at least a `/16` so that `/24` subnets can be
    /// carved from the third octet.
    pub fn new(parent: Ipv4Net) -> Self {
        Self {
            parent,
            in_use: Mutex::new(HashSet::new()),
        }
    }

    /// Allocate a `/24` subnet from the pool.
    ///
    /// Returns the subnet with the next available third-octet value. Subnets
    /// are allocated starting from `.1.0/24`, avoiding `.0.0/24` which is
    /// typically reserved.
    pub fn allocate(&self) -> Result<Ipv4Net, SubnetPoolError> {
        let mut in_use = self.in_use.lock().expect("subnet pool lock poisoned");
        // Skip 0 (reserved) and scan 1–255
        for third_octet in 1u8..=255 {
            if !in_use.contains(&third_octet) {
                in_use.insert(third_octet);
                return Ok(self.make_subnet(third_octet));
            }
        }
        Err(SubnetPoolError::Exhausted(self.parent))
    }

    /// Release a previously allocated subnet back to the pool.
    pub fn release(&self, subnet: Ipv4Net) {
        let third_octet = self.third_octet_of(subnet.network());
        let mut in_use = self.in_use.lock().expect("subnet pool lock poisoned");
        in_use.remove(&third_octet);
    }

    /// Build a /24 subnet using the parent's first two octets and the given
    /// third octet.
    fn make_subnet(&self, third_octet: u8) -> Ipv4Net {
        let parent_octets = self.parent.network().octets();
        let addr = Ipv4Addr::new(parent_octets[0], parent_octets[1], third_octet, 0);
        Ipv4Net::new(addr, 24).expect("prefix length 24 is always valid")
    }

    /// Extract the third octet from an address (used when releasing).
    fn third_octet_of(&self, addr: Ipv4Addr) -> u8 {
        addr.octets()[2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_returns_unique_subnets() {
        let pool = SubnetPool::new("10.200.0.0/16".parse().unwrap());
        let a = pool.allocate().unwrap();
        let b = pool.allocate().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.prefix_len(), 24);
        assert_eq!(b.prefix_len(), 24);
    }

    #[test]
    fn release_allows_reallocation() {
        let pool = SubnetPool::new("10.200.0.0/16".parse().unwrap());
        let a = pool.allocate().unwrap();
        pool.release(a);
        let b = pool.allocate().unwrap();
        // Should get the same subnet back
        assert_eq!(a, b);
    }

    #[test]
    fn skips_zero_subnet() {
        let pool = SubnetPool::new("10.200.0.0/16".parse().unwrap());
        let first = pool.allocate().unwrap();
        // Third octet should be 1, not 0
        assert_eq!(first.network().octets()[2], 1);
    }

    #[test]
    fn exhaustion_returns_error() {
        let pool = SubnetPool::new("10.200.0.0/16".parse().unwrap());
        // Allocate all 255 subnets (octets 1–255)
        for _ in 0..255 {
            pool.allocate().unwrap();
        }
        assert!(pool.allocate().is_err());
    }
}
