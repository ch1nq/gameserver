//! Finished-match history and Weng-Lin ratings (`matches`, `match_placements`,
//! `agent_ratings`). Only `Finished` games are recorded — failed games never
//! touch ratings. Ratings are Elo-scale Weng-Lin (new players 1500 ± 500).

use std::collections::HashMap;

use common::{AgentId, FinishedPlacement, MatchRecorder, StoredRating};
use sqlx::{PgPool, Row};

use crate::agents::agent::{Agent, AgentImageUrl, AgentName, AgentStatus};
use crate::users::{UserId, Username};

/// Canonical rating defaults live in `common` (single source of truth);
/// re-exported here so callers don't need a second import.
pub use common::{DEFAULT_RATING, DEFAULT_UNCERTAINTY, PROVISIONAL_MATCHES};

#[derive(Debug, Clone)]
pub struct MatchManager {
    db_pool: PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum MatchManagerError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("cannot record a match with no placements")]
    EmptyPlacements,
    #[error("duplicate agent {0} in match placements")]
    DuplicateAgent(AgentId),
}

/// One leaderboard row: agent + owner + current rating.
#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub agent: Agent,
    pub username: Username,
    pub rating: f64,
    pub uncertainty: f64,
    pub matches_played: i32,
    pub wins: i32,
}

impl LeaderboardEntry {
    /// Still calibrating: fewer than [`PROVISIONAL_MATCHES`] games.
    pub fn is_provisional(&self) -> bool {
        StoredRating::is_provisional(self.matches_played)
    }

    /// Display rating, formatted identically to `ranking::format_rating`.
    pub fn formatted_rating(&self) -> String {
        StoredRating {
            rating: self.rating,
            uncertainty: self.uncertainty,
        }
        .format()
    }
}

impl MatchManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Current ratings for `agent_ids`. Agents without a row get the default
    /// (1500 ± 500) so first-match math needs no special casing.
    pub async fn get_ratings(
        &self,
        agent_ids: &[AgentId],
    ) -> Result<HashMap<AgentId, StoredRating>, MatchManagerError> {
        let mut out: HashMap<AgentId, StoredRating> = agent_ids
            .iter()
            .map(|id| {
                (
                    *id,
                    StoredRating {
                        rating: DEFAULT_RATING,
                        uncertainty: DEFAULT_UNCERTAINTY,
                    },
                )
            })
            .collect();
        if agent_ids.is_empty() {
            return Ok(out);
        }
        let rows = sqlx::query(
            r#"
            SELECT agent_id, rating, uncertainty
            FROM agent_ratings
            WHERE agent_id = ANY($1)
            "#,
        )
        .bind(agent_ids)
        .fetch_all(&self.db_pool)
        .await?;
        for row in rows {
            let agent_id: AgentId = row.try_get("agent_id")?;
            let rating: f64 = row.try_get("rating")?;
            let uncertainty: f64 = row.try_get("uncertainty")?;
            out.insert(
                agent_id,
                StoredRating {
                    rating,
                    uncertainty,
                },
            );
        }
        Ok(out)
    }

    /// Persist one finished match and upsert current ratings, atomically.
    ///
    /// `placements` carries before/after snapshots (computed by the caller via
    /// `achtung-ranking`); a win is `position == 1`, so tied winners each
    /// record a win. Returns the `matches.id`.
    ///
    /// Fails fast on an empty list (no orphan `matches` row) or a duplicate
    /// `agent_id` (which would otherwise surface as an opaque primary-key
    /// violation mid-transaction).
    pub async fn record_finished_match(
        &self,
        external_id: &str,
        placements: &[FinishedPlacement],
    ) -> Result<i64, MatchManagerError> {
        if placements.is_empty() {
            return Err(MatchManagerError::EmptyPlacements);
        }
        {
            let mut seen = std::collections::HashSet::with_capacity(placements.len());
            for p in placements {
                if !seen.insert(p.agent_id) {
                    return Err(MatchManagerError::DuplicateAgent(p.agent_id));
                }
            }
        }
        let mut tx = self.db_pool.begin().await?;
        let match_id: i64 = sqlx::query(
            r#"
            INSERT INTO matches (external_id, num_agents, status)
            VALUES ($1, $2, 'finished')
            RETURNING id
            "#,
        )
        .bind(external_id)
        .bind(placements.len() as i32)
        .fetch_one(&mut *tx)
        .await?
        .try_get("id")?;

        for p in placements {
            sqlx::query(
                r#"
                INSERT INTO match_placements
                    (match_id, agent_id, position, score,
                     rating_before, uncertainty_before, rating_after, uncertainty_after)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(match_id)
            .bind(p.agent_id)
            .bind(p.position as i32)
            .bind(p.score as i32)
            .bind(p.old_rating.rating)
            .bind(p.old_rating.uncertainty)
            .bind(p.new_rating.rating)
            .bind(p.new_rating.uncertainty)
            .execute(&mut *tx)
            .await?;

            let won = p.position == 1;
            sqlx::query(
                r#"
                INSERT INTO agent_ratings (agent_id, rating, uncertainty, matches_played, wins)
                VALUES ($1, $2, $3, 1, CASE WHEN $4 THEN 1 ELSE 0 END)
                ON CONFLICT (agent_id) DO UPDATE SET
                    rating = EXCLUDED.rating,
                    uncertainty = EXCLUDED.uncertainty,
                    matches_played = agent_ratings.matches_played + 1,
                    wins = agent_ratings.wins + CASE WHEN $4 THEN 1 ELSE 0 END,
                    updated_at = now()
                "#,
            )
            .bind(p.agent_id)
            .bind(p.new_rating.rating)
            .bind(p.new_rating.uncertainty)
            .bind(won)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        tracing::info!(match_id = match_id, "Recorded finished match");
        Ok(match_id)
    }

    /// All agents with their current rating, best first. Agents that never
    /// played sort as default (1500). Used by the landing leaderboard.
    pub async fn get_leaderboard(&self) -> Result<Vec<LeaderboardEntry>, MatchManagerError> {
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.name, a.user_id, a.status, a.image_url, u.username,
                   COALESCE(r.rating, $1) AS rating,
                   COALESCE(r.uncertainty, $2) AS uncertainty,
                   COALESCE(r.matches_played, 0) AS matches_played,
                   COALESCE(r.wins, 0) AS wins
            FROM agents a
            JOIN users u ON u.id = a.user_id
            LEFT JOIN agent_ratings r ON r.agent_id = a.id
            ORDER BY rating DESC, uncertainty ASC, a.id DESC
            "#,
        )
        .bind(DEFAULT_RATING)
        .bind(DEFAULT_UNCERTAINTY)
        .fetch_all(&self.db_pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: AgentId = row.try_get("id")?;
            let name: String = row.try_get("name")?;
            let user_id: UserId = row.try_get("user_id")?;
            let status: AgentStatus = row.try_get("status")?;
            let image_url_str: String = row.try_get("image_url")?;
            let username: Username = row.try_get("username")?;
            let image_url = AgentImageUrl::parse_full(&image_url_str, user_id)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            out.push(LeaderboardEntry {
                agent: Agent {
                    id,
                    name: AgentName::from(name),
                    user_id,
                    status,
                    image_url,
                },
                username,
                rating: row.try_get("rating")?,
                uncertainty: row.try_get("uncertainty")?,
                matches_played: row.try_get("matches_played")?,
                wins: row.try_get("wins")?,
            });
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl MatchRecorder for MatchManager {
    async fn load_ratings(
        &self,
        agent_ids: &[AgentId],
    ) -> Result<HashMap<AgentId, StoredRating>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.get_ratings(agent_ids).await?)
    }

    async fn record_finished_match(
        &self,
        external_match_id: &str,
        placements: &[FinishedPlacement],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.record_finished_match(external_match_id, placements)
            .await?;
        Ok(())
    }
}
