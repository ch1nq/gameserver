use std::collections::HashMap;

use crate::{AgentId, AgentImageUrl, ContainerImageUrl, RegistryToken};

/// Agent info needed for a match
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: AgentId,
    pub name: String,
    pub image_url: AgentImageUrl,
}

/// Trait for fetching active agents from the database
#[async_trait::async_trait]
pub trait AgentRepository: Send + Sync {
    /// Get N random active agents for a match
    async fn get_random_active_agents(
        &self,
        count: usize,
    ) -> Result<Vec<AgentInfo>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Trait for generating scoped deploy tokens for pulling images from the registry
#[async_trait::async_trait]
pub trait DeployTokenProvider: Send + Sync {
    /// Get a short-lived token with pull access to the given container image
    ///
    /// Accepts any type implementing `ContainerImageUrl` (e.g., `AgentImageUrl`, `ImageUrl`).
    /// The repository is extracted from the image URL internally via the trait method.
    async fn get_deploy_token(
        &self,
        image: &(dyn ContainerImageUrl + Send + Sync),
    ) -> Result<RegistryToken, Box<dyn std::error::Error + Send + Sync>>;
}

/// Canonical default mu for an agent with no recorded matches.
///
/// Raw Weng-Lin scale (matches `skillratings::WengLinRating::new()`).
/// Defined here so `core` and `coordinator` share one source of truth;
/// `achtung-ranking` keeps a mirrored literal to stay a pure math leaf
/// without depending on this crate (see the sync test in `coordinator`).
pub const DEFAULT_RATING: f64 = 25.0;
/// Canonical default sigma for an agent with no recorded matches (25/3).
pub const DEFAULT_UNCERTAINTY: f64 = 25.0 / 3.0;
/// Matches played below this count render as provisional on the leaderboard.
pub const PROVISIONAL_MATCHES: i32 = 20;

/// Current Weng-Lin rating of one agent (raw scale: 25.0 ± 8.33 for new
/// players). Plain floats so `core` and `coordinator` share the shape without
/// depending on the `skillratings` crate directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoredRating {
    pub rating: f64,
    pub uncertainty: f64,
}

impl StoredRating {
    /// Default rating for agents with no history.
    pub fn default_rating() -> Self {
        Self {
            rating: DEFAULT_RATING,
            uncertainty: DEFAULT_UNCERTAINTY,
        }
    }

    /// Human-readable raw rating, e.g. `"24.1 ± 3.2"`.
    pub fn format(&self) -> String {
        format_rating(self.rating, self.uncertainty)
    }

    /// Agents below [`PROVISIONAL_MATCHES`] games are still calibrating.
    pub fn is_provisional(matches_played: i32) -> bool {
        matches_played < PROVISIONAL_MATCHES
    }
}

/// Human-readable raw rating, e.g. `"24.1 ± 3.2"`. Shared helper so the
/// website and the rating crate format identically.
pub fn format_rating(rating: f64, uncertainty: f64) -> String {
    format!("{:.1} ± {:.1}", rating, uncertainty)
}

/// One placement with before/after rating snapshots, ready to persist.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FinishedPlacement {
    pub agent_id: AgentId,
    pub position: u32,
    pub score: u32,
    pub old_rating: StoredRating,
    pub new_rating: StoredRating,
}

/// Trait for loading and persisting Weng-Lin ratings. Implemented by
/// `achtung-core`'s `MatchManager`; consumed by the coordinator after a
/// `Finished` game. Failed games never reach this trait.
#[async_trait::async_trait]
pub trait MatchRecorder: Send + Sync {
    /// Current ratings for `agent_ids`; agents with no history get the
    /// default rating from the implementation.
    async fn load_ratings(
        &self,
        agent_ids: &[AgentId],
    ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>>;

    /// Persist one finished match (history rows + current-rating upserts).
    async fn record_finished_match(
        &self,
        external_match_id: &str,
        placements: &[FinishedPlacement],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
