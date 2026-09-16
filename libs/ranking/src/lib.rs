//! Weng-Lin (OpenSkill) rating calculations for free-for-all matches.
//!
//! Pure functions only — no database access. The coordinator loads current
//! ratings, calls [`rate_ffa`], then persists the result via `achtung-core`.
//! Keeping the `skillratings` dependency isolated here means version churn
//! touches a single crate.
//!
//! Elo-scale Weng-Lin: raw OpenSkill units times 60, so new players start
//! at mu=1500, sigma=500 — the familiar Elo feel with identical dynamics
//! (the update equations are homogeneous of degree 1 in mu/sigma/beta).
//! `skillratings`' raw constructors (`WengLinRating::new`,
//! `WengLinConfig::new`) are never used here; see [`default_rating`] and
//! [`default_config`].

pub use skillratings::MultiTeamOutcome;
pub use skillratings::weng_lin::{WengLinConfig, WengLinRating};

/// Default mu for a player with no recorded matches (raw 25.0 × 60).
///
/// Mirrored from `common::DEFAULT_RATING` (this crate stays a pure math
/// leaf without depending on `common`; a sync test in `coordinator`
/// asserts they match).
pub const DEFAULT_RATING: f64 = 1500.0;
/// Default sigma for a player with no recorded matches (raw 25/3 × 60).
/// Mirrored from `common::DEFAULT_UNCERTAINTY` (see above).
pub const DEFAULT_UNCERTAINTY: f64 = 500.0;

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

/// Default calculation config for the Elo-scale ratings above: raw
/// OpenSkill beta (25/6) times 60, so skill-class width stays one
/// default-sigma step just like upstream.
///
/// `uncertainty_tolerance` is intentionally *not* scaled: it floors the
/// dimensionless multiplier `(1 − eta)` inside `skillratings`, so it is
/// scale-invariant by construction. Do not use `WengLinConfig::new()` —
/// its raw beta would saturate every `p_value` at this scale and cause
/// wild rating swings.
pub fn default_config() -> WengLinConfig {
    WengLinConfig {
        beta: 250.0,
        uncertainty_tolerance: 0.000_001,
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

/// Human-readable Elo-scale mean, e.g. `"1500"`. Uncertainty is
/// deliberately kept out of the display; it remains stored and drives the
/// rating math. Formats identically to `common::format_rating` (kept local
/// so this crate has no dependency on `common`).
pub fn format_rating(rating: &WengLinRating) -> String {
    format!("{:.0}", rating.rating)
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
            rating: default_rating(),
            rank: rank(position),
        }
    }

    #[test]
    fn new_players_start_at_default() {
        let r = default_rating();
        assert!((r.rating - DEFAULT_RATING).abs() < f64::EPSILON);
        assert!((r.uncertainty - DEFAULT_UNCERTAINTY).abs() < 1e-12);
        assert_eq!(r.rating, 1500.0);
        assert_eq!(r.uncertainty, 500.0);
        // Guard rail: the raw upstream constructor must never leak in as a
        // default — ratings live at Elo scale (raw × 60).
        assert_ne!(default_rating(), WengLinRating::new());
    }

    #[test]
    fn default_config_scales_beta_not_tolerance() {
        let config = default_config();
        assert_eq!(config.beta, 250.0);
        assert_eq!(config.uncertainty_tolerance, 0.000_001);
    }

    #[test]
    fn rank_rejects_zero() {
        assert!(Rank::new(0).is_none());
        assert_eq!(Rank::new(1).unwrap().get(), 1);
    }

    #[test]
    fn winner_gains_loser_loses_and_uncertainty_shrinks() {
        let players = vec![player(1, 1), player(2, 2)];
        let out = rate_ffa(&players, &default_config()).unwrap();
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
        let config = default_config();
        let strong = rating(2100.0, 120.0);
        let weak = rating(900.0, 120.0);

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
        let out = rate_ffa(&players, &default_config()).unwrap();
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
        let config = default_config();
        let decisive = rate_ffa(&[player(1, 1), player(2, 2)], &config).unwrap();
        let tied = rate_ffa(&[player(1, 1), player(2, 1)], &config).unwrap();
        let decisive_delta = (decisive[0].new_rating.rating - decisive[0].old_rating.rating).abs();
        let tie_delta = (tied[0].new_rating.rating - tied[0].old_rating.rating).abs();
        assert!(tie_delta < decisive_delta);
    }

    #[test]
    fn rejects_too_few_players_and_duplicates() {
        assert_eq!(
            rate_ffa(&[], &default_config()),
            Err(RatingError::NotEnoughPlayers(0))
        );
        assert_eq!(
            rate_ffa(&[player(1, 1)], &default_config()),
            Err(RatingError::NotEnoughPlayers(1))
        );
        // A zero rank cannot be constructed: the type makes it unrepresentable.
        // The error variant survives for wire parsing at the boundary.
        assert_eq!(
            RatingError::InvalidRank(0, 1).to_string(),
            "rank must be >= 1, got 0 for agent 1"
        );
        assert_eq!(
            rate_ffa(&[player(1, 1), player(1, 2)], &default_config()),
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
    fn format_rating_renders_mean_only() {
        assert_eq!(format_rating(&default_rating()), "1500");
    }

    #[test]
    fn scaled_dynamics_match_raw_openskill_times_sixty() {
        // Homogeneity lock: Elo-scale math must equal raw OpenSkill math
        // × 60, proving the rescale preserved upstream dynamics exactly.
        let raw = vec![
            (1i64, 25.0, 25.0 / 3.0, 1u32),
            (2, 30.0, 5.0, 2),
            (3, 20.0, 6.0, 2),
            (4, 15.0, 2.0, 4),
        ];
        let scaled_players: Vec<FfaPlayer> = raw
            .iter()
            .map(|(id, mu, sigma, pos)| FfaPlayer {
                agent_id: *id,
                rating: rating(mu * 60.0, sigma * 60.0),
                rank: rank(*pos),
            })
            .collect();
        let scaled_out = rate_ffa(&scaled_players, &default_config()).unwrap();

        let raw_singles: Vec<[WengLinRating; 1]> = raw
            .iter()
            .map(|(_, mu, sigma, _)| [rating(*mu, *sigma)])
            .collect();
        let raw_teams: Vec<(&[WengLinRating], MultiTeamOutcome)> = raw_singles
            .iter()
            .zip(raw.iter())
            .map(|(team, (_, _, _, pos))| (&team[..], MultiTeamOutcome::new(*pos as usize)))
            .collect();
        let raw_out =
            skillratings::weng_lin::weng_lin_multi_team(&raw_teams, &WengLinConfig::new());

        for (s, r) in scaled_out.iter().zip(raw_out.iter()) {
            let rel = |a: f64, b: f64| (a - b * 60.0).abs() / (b * 60.0).abs();
            assert!(rel(s.new_rating.rating, r[0].rating) < 1e-12);
            assert!(rel(s.new_rating.uncertainty, r[0].uncertainty) < 1e-12);
        }
    }
}
