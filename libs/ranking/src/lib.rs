//! Weng-Lin (OpenSkill) rating calculations for free-for-all matches.
//!
//! Pure functions only — no database access. The coordinator loads current
//! ratings, calls [`rate_ffa`], then persists the result via `achtung-core`.
//! Keeping the `skillratings` dependency isolated here means version churn
//! touches a single crate.
//!
//! Scale is raw Weng-Lin: new players start at mu=25.0, sigma=25/3≈8.33.
//! Display the raw values (see [`format_rating`]) rather than mapping to 1200.

pub use skillratings::MultiTeamOutcome;
pub use skillratings::weng_lin::{WengLinConfig, WengLinRating};

/// Default mu for a player with no recorded matches.
///
/// Mirrored from `common::DEFAULT_RATING` (this crate stays a pure math
/// leaf without depending on `common`; a sync test in `coordinator`
/// asserts they match).
pub const DEFAULT_RATING: f64 = 25.0;
/// Default sigma for a player with no recorded matches (25/3).
/// Mirrored from `common::DEFAULT_UNCERTAINTY` (see above).
pub const DEFAULT_UNCERTAINTY: f64 = 25.0 / 3.0;

/// 1-based placement. Non-zero by construction: a `Rank` cannot represent
/// the proto default `0`, so `rate_ffa` never has to defensively re-check
/// the lower bound. Equal ranks represent ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rank(std::num::NonZeroU32);

impl Rank {
    /// 1-based placement, or `None` for `0`.
    pub fn new(position: u32) -> Option<Self> {
        std::num::NonZeroU32::new(position).map(Self)
    }

    /// 1-based placement as stored on the wire.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl From<Rank> for u32 {
    fn from(rank: Rank) -> Self {
        rank.get()
    }
}

impl From<Rank> for usize {
    fn from(rank: Rank) -> Self {
        rank.get() as usize
    }
}

/// One entrant in a finished free-for-all match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FfaPlayer {
    /// Matches `common::AgentId` (`i64`) without depending on `common`,
    /// so this crate stays a pure math leaf.
    pub agent_id: i64,
    pub rating: WengLinRating,
    /// 1-based placement. Equal ranks represent ties.
    pub rank: Rank,
}

/// Rating update for one entrant, in the same order as the input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RatedPlayer {
    pub agent_id: i64,
    pub rank: Rank,
    pub old_rating: WengLinRating,
    pub new_rating: WengLinRating,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RatingError {
    #[error("need at least 2 players, got {0}")]
    NotEnoughPlayers(usize),
    #[error("rank must be >= 1, got {0} for agent {1}")]
    InvalidRank(u32, i64),
    #[error("duplicate agent {0} in match placements")]
    DuplicateAgent(i64),
}

/// A placement list checked once at the boundary: at least 2 players and
/// no duplicate `agent_id`. Lets callers fail fast (e.g. before loading
/// ratings) instead of discovering a corrupt list mid-pipeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValidatedFfaPlayers<'a> {
    players: &'a [FfaPlayer],
}

impl<'a> TryFrom<&'a [FfaPlayer]> for ValidatedFfaPlayers<'a> {
    type Error = RatingError;

    fn try_from(players: &'a [FfaPlayer]) -> Result<Self, Self::Error> {
        validate_players(players)?;
        Ok(Self { players })
    }
}

impl<'a> ValidatedFfaPlayers<'a> {
    /// The checked players, in the original order.
    pub fn as_slice(&self) -> &'a [FfaPlayer] {
        self.players
    }
}

fn validate_players(players: &[FfaPlayer]) -> Result<(), RatingError> {
    if players.len() < 2 {
        return Err(RatingError::NotEnoughPlayers(players.len()));
    }
    // `agent_id` must be unique: duplicates would otherwise pair one agent
    // with two rating updates and fail opaquely at the DB primary key.
    let mut seen = std::collections::HashSet::with_capacity(players.len());
    for p in players {
        if !seen.insert(p.agent_id) {
            return Err(RatingError::DuplicateAgent(p.agent_id));
        }
    }
    Ok(())
}

/// Default rating for agents with no history.
pub fn default_rating() -> WengLinRating {
    WengLinRating {
        rating: DEFAULT_RATING,
        uncertainty: DEFAULT_UNCERTAINTY,
    }
}

/// Compute new Weng-Lin ratings for a free-for-all match.
///
/// Each agent is treated as a one-person team and ranked by `rank`
/// (1 = winner). Agents sharing a rank are scored as tied by the
/// underlying `weng_lin_multi_team` call.
///
/// Returns updates in input order. Rank validity (`>= 1`) is guaranteed
/// by the [`Rank`] type; the remaining list invariants (at least 2
/// players, unique agents) are checked here — or upfront via
/// [`ValidatedFfaPlayers::try_from`] to fail fast before doing I/O.
pub fn rate_ffa(
    players: &[FfaPlayer],
    config: &WengLinConfig,
) -> Result<Vec<RatedPlayer>, RatingError> {
    validate_players(players)?;

    // One single-player team per agent; borrow ratings without cloning the vec.
    let singles: Vec<[WengLinRating; 1]> = players.iter().map(|p| [p.rating]).collect();
    let teams_and_ranks: Vec<(&[WengLinRating], MultiTeamOutcome)> = singles
        .iter()
        .zip(players.iter())
        .map(|(team, p)| (&team[..], MultiTeamOutcome::new(usize::from(p.rank))))
        .collect();

    let new_teams = skillratings::weng_lin::weng_lin_multi_team(&teams_and_ranks, config);

    Ok(players
        .iter()
        .zip(new_teams.iter())
        .map(|(p, team)| RatedPlayer {
            agent_id: p.agent_id,
            rank: p.rank,
            old_rating: p.rating,
            new_rating: team[0],
        })
        .collect())
}

/// Human-readable raw rating, e.g. `"24.1 ± 3.2"`.
/// Formats identically to `common::format_rating` (kept local so this
/// crate has no dependency on `common`).
pub fn format_rating(rating: &WengLinRating) -> String {
    format!("{:.1} ± {:.1}", rating.rating, rating.uncertainty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rating(mu: f64, sigma: f64) -> WengLinRating {
        WengLinRating {
            rating: mu,
            uncertainty: sigma,
        }
    }

    fn rank(position: u32) -> Rank {
        Rank::new(position).expect("test rank must be >= 1")
    }

    fn player(agent_id: i64, position: u32) -> FfaPlayer {
        FfaPlayer {
            agent_id,
            rating: WengLinRating::new(),
            rank: rank(position),
        }
    }

    #[test]
    fn new_players_start_at_default() {
        let r = WengLinRating::new();
        assert!((r.rating - DEFAULT_RATING).abs() < f64::EPSILON);
        assert!((r.uncertainty - DEFAULT_UNCERTAINTY).abs() < 1e-12);
        assert_eq!(default_rating(), WengLinRating::new());
    }

    #[test]
    fn rank_rejects_zero() {
        assert!(Rank::new(0).is_none());
        assert_eq!(Rank::new(1).unwrap().get(), 1);
    }

    #[test]
    fn winner_gains_loser_loses_and_uncertainty_shrinks() {
        let players = vec![player(1, 1), player(2, 2)];
        let out = rate_ffa(&players, &WengLinConfig::new()).unwrap();
        assert_eq!(out.len(), 2);
        // Input order preserved.
        assert_eq!(out[0].agent_id, 1);
        assert_eq!(out[1].agent_id, 2);
        assert!(out[0].new_rating.rating > players[0].rating.rating);
        assert!(out[1].new_rating.rating < players[1].rating.rating);
        assert!(out[0].new_rating.uncertainty < players[0].rating.uncertainty);
    }

    #[test]
    fn underdog_win_moves_more_than_favorite_win() {
        let config = WengLinConfig::new();
        let strong = rating(35.0, 2.0);
        let weak = rating(15.0, 2.0);

        let upset = rate_ffa(
            &[
                FfaPlayer {
                    agent_id: 1,
                    rating: weak,
                    rank: rank(1),
                },
                FfaPlayer {
                    agent_id: 2,
                    rating: strong,
                    rank: rank(2),
                },
            ],
            &config,
        )
        .unwrap();
        let upset_gain = upset[0].new_rating.rating - weak.rating;

        let expected = rate_ffa(
            &[
                FfaPlayer {
                    agent_id: 1,
                    rating: strong,
                    rank: rank(1),
                },
                FfaPlayer {
                    agent_id: 2,
                    rating: weak,
                    rank: rank(2),
                },
            ],
            &config,
        )
        .unwrap();
        let favorite_gain = expected[0].new_rating.rating - strong.rating;

        assert!(upset_gain > favorite_gain);
        assert!(upset_gain > 0.0 && favorite_gain > 0.0);
    }

    #[test]
    fn four_player_ffa_is_zero_sum_ordered() {
        let players = vec![player(1, 1), player(2, 2), player(3, 3), player(4, 4)];
        let out = rate_ffa(&players, &WengLinConfig::new()).unwrap();
        let deltas: Vec<f64> = out
            .iter()
            .map(|r| r.new_rating.rating - r.old_rating.rating)
            .collect();
        assert!(deltas[0] > deltas[1] && deltas[1] > deltas[2] && deltas[2] > deltas[3]);
        assert!(deltas[0] > 0.0 && deltas[3] < 0.0);
        // Zero-sum: total mu is conserved.
        let total: f64 = deltas.iter().sum();
        assert!(total.abs() < 1e-9, "expected zero-sum, got {total}");
    }

    #[test]
    fn tie_moves_less_than_decisive_result() {
        let config = WengLinConfig::new();
        let decisive = rate_ffa(&[player(1, 1), player(2, 2)], &config).unwrap();
        let tied = rate_ffa(&[player(1, 1), player(2, 1)], &config).unwrap();
        let decisive_delta = (decisive[0].new_rating.rating - decisive[0].old_rating.rating).abs();
        let tie_delta = (tied[0].new_rating.rating - tied[0].old_rating.rating).abs();
        assert!(tie_delta < decisive_delta);
    }

    #[test]
    fn rejects_too_few_players_and_duplicates() {
        assert_eq!(
            rate_ffa(&[], &WengLinConfig::new()),
            Err(RatingError::NotEnoughPlayers(0))
        );
        assert_eq!(
            rate_ffa(&[player(1, 1)], &WengLinConfig::new()),
            Err(RatingError::NotEnoughPlayers(1))
        );
        // A zero rank cannot be constructed: the type makes it unrepresentable.
        // The error variant survives for wire parsing at the boundary.
        assert_eq!(
            RatingError::InvalidRank(0, 1).to_string(),
            "rank must be >= 1, got 0 for agent 1"
        );
        assert_eq!(
            rate_ffa(&[player(1, 1), player(1, 2)], &WengLinConfig::new()),
            Err(RatingError::DuplicateAgent(1))
        );
    }

    #[test]
    fn validated_players_fails_fast_before_io() {
        let players = vec![player(1, 1), player(1, 2)];
        assert_eq!(
            ValidatedFfaPlayers::try_from(players.as_slice()),
            Err(RatingError::DuplicateAgent(1))
        );
        let solo = vec![player(1, 1)];
        assert_eq!(
            ValidatedFfaPlayers::try_from(solo.as_slice()),
            Err(RatingError::NotEnoughPlayers(1))
        );
        let ok = vec![player(1, 1), player(2, 2)];
        assert_eq!(
            ValidatedFfaPlayers::try_from(ok.as_slice())
                .unwrap()
                .as_slice(),
            ok.as_slice()
        );
    }

    #[test]
    fn format_rating_renders_raw_scale() {
        assert_eq!(format_rating(&WengLinRating::new()), "25.0 ± 8.3");
    }
}
