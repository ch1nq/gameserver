//! Typed match slots separating the game host from agents.
//!
//! [`AgentSlot`] names one agent by 0-based index; the host has no slot and is
//! spawned through a dedicated host path. [`MatchLayout`] validates the agent
//! count once in `init_match`, so backends offer `spawn_host` / `spawn_agent`
//! with no role branch left to get wrong.

use std::num::NonZeroU8;
use std::ops::RangeInclusive;

use crate::MachineError;

/// 0-based agent index. The host is unrepresentable here by construction.
///
/// The on-the-wire slot number (`base + N` relay ports) is
/// [`Self::raw_slot`]: `index + 1`. Slot 0 is always the game host and never
/// surfaces as a value of this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentSlot(u8);

impl AgentSlot {
    /// Build from a 0-based agent index (0 == first agent).
    pub fn from_index(index: usize) -> Option<Self> {
        u8::try_from(index).ok().map(Self)
    }

    /// Build from a raw wire slot (`1+`). Returns `None` for 0 (the host).
    pub fn from_raw_slot(raw: u8) -> Option<Self> {
        raw.checked_sub(1).map(Self)
    }

    /// 0-based agent index.
    pub fn index(self) -> u8 {
        self.0
    }

    /// Raw wire slot: `index + 1`. Never 0.
    pub fn raw_slot(self) -> u8 {
        // `u8 + 1` cannot overflow: max index is 254 (see `MatchLayout::new`),
        // so max raw is 255.
        self.0 + 1
    }
}

/// Validated shape of a match: how many agents will spawn.
///
/// Constructed once per match and passed to `init_match`, so the
/// zero-agent case, `usize -> u8` truncation, and host-port overflow are
/// rejected up front instead of at every spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchLayout {
    num_agents: NonZeroU8,
}

impl MatchLayout {
    /// Validate an agent count. Rejects 0 (a match is host + >=1 agent) and
    /// counts above 254 (host + 255 agents would be 256 slots, which does not
    /// fit the `u8` wire slot).
    pub fn new(num_agents: usize) -> Result<Self, MachineError> {
        let n = u8::try_from(num_agents).map_err(|_| {
            MachineError::MatchInit(format!(
                "agent count {num_agents} exceeds the 254-agent limit"
            ))
        })?;
        if n > 254 {
            return Err(MachineError::MatchInit(format!(
                "agent count {num_agents} exceeds the 254-agent limit"
            )));
        }
        let num_agents = NonZeroU8::new(n).ok_or_else(|| {
            MachineError::MatchInit("a match needs at least one agent".to_string())
        })?;
        Ok(Self { num_agents })
    }

    /// Number of agents in the match (>= 1).
    pub fn num_agents(self) -> u8 {
        self.num_agents.get()
    }

    /// Total machines: game host + agents.
    pub fn num_slots(self) -> u8 {
        // `num_agents <= 255`, but `num_agents == 255` would make 256 slots,
        // which does not fit in `u8`. Cap the agent count so this holds.
        // (Enforced in `new`; debug-assert here as backstop.)
        debug_assert!(self.num_agents.get() <= 254);
        self.num_agents.get() + 1
    }

    /// The slot for the `index`-th agent (0-based), or `None` if out of range.
    pub fn agent_slot(self, index: usize) -> Option<AgentSlot> {
        AgentSlot::from_index(index).filter(|s| s.index() < self.num_agents())
    }

    /// All agent slots in order.
    pub fn all_agent_slots(self) -> impl ExactSizeIterator<Item = AgentSlot> {
        (0..self.num_agents.get()).map(AgentSlot)
    }

    /// Relay port range fronting the agents: `base+1 ..= base+num_agents`.
    ///
    /// Always non-empty (zero-agent matches are rejected in `new`), so callers
    /// no longer branch on `num_slots > 1`. Returns a `MatchInit` error when
    /// the range would overflow `u16` instead of silently colliding ports.
    pub fn relay_range(self, base: u16) -> Result<RangeInclusive<u16>, MachineError> {
        let lo = base.checked_add(1).ok_or_else(|| {
            MachineError::MatchInit(format!("relay port base {base} is at the u16 limit"))
        })?;
        let hi = base
            .checked_add(u16::from(self.num_agents.get()))
            .ok_or_else(|| {
                MachineError::MatchInit(format!(
                    "relay ports {base}+1..={} overflow u16",
                    self.num_agents.get()
                ))
            })?;
        Ok(lo..=hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_agents() {
        assert!(MatchLayout::new(0).is_err());
    }

    #[test]
    fn rejects_more_than_254_agents_so_slots_fit_in_u8() {
        // 254 agents -> 255 slots fits; 255 agents -> 256 slots does not.
        assert!(MatchLayout::new(254).is_ok());
        assert!(MatchLayout::new(255).is_err());
        assert!(MatchLayout::new(10_000).is_err());
    }

    #[test]
    fn raw_slot_is_index_plus_one_and_never_zero() {
        let slot = AgentSlot::from_index(0).unwrap();
        assert_eq!(slot.raw_slot(), 1);
        assert_eq!(AgentSlot::from_index(2).unwrap().raw_slot(), 3);
        assert_eq!(AgentSlot::from_raw_slot(0), None);
        assert_eq!(
            AgentSlot::from_raw_slot(3).unwrap(),
            AgentSlot::from_index(2).unwrap()
        );
    }

    #[test]
    fn agent_slot_lookup_is_bounded_by_the_layout() {
        let layout = MatchLayout::new(2).unwrap();
        assert_eq!(layout.agent_slot(0).unwrap().raw_slot(), 1);
        assert_eq!(layout.agent_slot(1).unwrap().raw_slot(), 2);
        assert_eq!(layout.agent_slot(2), None);
        assert_eq!(layout.all_agent_slots().len(), 2);
        assert_eq!(layout.num_slots(), 3);
    }

    #[test]
    fn relay_range_covers_exactly_the_agent_ports() {
        let layout = MatchLayout::new(3).unwrap();
        assert_eq!(layout.relay_range(51000).unwrap(), 51001..=51003);
    }

    #[test]
    fn relay_range_rejects_u16_overflow() {
        let layout = MatchLayout::new(3).unwrap();
        assert!(layout.relay_range(u16::MAX).is_err());
        assert!(layout.relay_range(u16::MAX - 2).is_err());
    }
}
