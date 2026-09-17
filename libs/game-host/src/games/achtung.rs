use rand::prelude::Distribution;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::game;

#[derive(Debug, Clone)]
pub struct AchtungConfig {
    pub arena_width: u32,
    pub arena_height: u32,
    pub edge_wrapping: bool,
}

impl Default for AchtungConfig {
    fn default() -> Self {
        Self {
            arena_width: 1000,
            arena_height: 200,
            edge_wrapping: false,
        }
    }
}

pub type PlayerId = usize;

#[derive(Serialize, Deserialize)]
pub enum GameEvent {
    UpdateState(Achtung),
    PlayerDied(PlayerId),
    PlayerJoined(PlayerId),
    GameOver { winner: Option<PlayerId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GameAction {
    Left,
    Right,
    Forward,
    // More like use item, etc.
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
}

type BlobId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Blob {
    id: BlobId,
    size: f32,
    position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achtung {
    timestep: u64,
    players: HashMap<PlayerId, Player>,
    #[serde(skip)]
    config: AchtungConfig,
    /// Seed for the deterministic per-player gap schedule. Stored (and
    /// serialized) so a given seed always replays the same gaps, including
    /// across serde round-trips.
    seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Angle {
    radians: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Player {
    is_alive: bool,
    head: Blob,
    body: Vec<Blob>,
    direction: Angle,
    speed: f32,
    turning_speed: f32,
    size: f32,
    action: GameAction,
    /// Ticks until the next gap starts (counts down every tick, including
    /// while in a gap). Independent per player, so gaps are desynchronized.
    ticks_until_gap: u32,
    /// Ticks left in the current gap (`0` means trail is being drawn).
    gap_remaining: u32,
    /// Number of gaps started so far; salts the deterministic schedule so
    /// every gap interval is a fresh draw.
    gaps_started: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchtungDiff {
    timestep: u64,
    players: HashMap<PlayerId, PlayerDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDiff {
    #[serde(skip_serializing_if = "Option::is_none")]
    is_alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    head: Option<Blob>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    body: Vec<Blob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<Angle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turning_speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<GameAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ticks_until_gap: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gap_remaining: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gaps_started: Option<u32>,
}

const COLLISION_SELF_IGNORE_N_LATEST: usize = 10;

// Gap scheduling, mirroring the original game: each curve stops drawing at
// random, per-player intervals, leaving holes to escape through.
const GAP_LENGTH: u32 = 9;
const GAP_MIN_INTERVAL: u32 = 120;
const GAP_INTERVAL_SPREAD: u32 = 200; // next gap starts in 120..320 ticks
const INITIAL_GAP_DELAY_BASE: u32 = 40;
const INITIAL_GAP_DELAY_SPREAD: u32 = 120; // first gap starts in 40..160 ticks

/// Deterministic uniform draw in `base..base + spread` from `(seed,
/// player_id, nonce)`.
///
/// Stateless by design: the RNG is re-seeded per draw instead of being held
/// in game state, so the schedule survives serde round-trips and a given
/// seed always replays identically. Nonce `0` is the initial delay;
/// subsequent gaps use `gaps_started` (`>= 1`), keeping the two domains
/// disjoint.
fn sample_gap_delay(seed: u64, player_id: PlayerId, nonce: u64, base: u32, spread: u32) -> u32 {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    let mut key = seed
        .wrapping_add((player_id as u64).wrapping_mul(0x9E3779B97F4A7C15))
        .wrapping_add(nonce.wrapping_mul(0xBF58476D1CE4E5B9));
    // Avoid a degenerate all-zero stream for seed 0 / player 0 / nonce 0.
    key |= 0x9E3779B97F4A7C15;
    let mut rng = StdRng::seed_from_u64(key);
    base + rng.random_range(0..spread)
}

fn initial_gap_delay(seed: u64, player_id: PlayerId) -> u32 {
    sample_gap_delay(
        seed,
        player_id,
        0,
        INITIAL_GAP_DELAY_BASE,
        INITIAL_GAP_DELAY_SPREAD,
    )
}

fn next_gap_interval(seed: u64, player_id: PlayerId, gaps_started: u32) -> u32 {
    sample_gap_delay(
        seed,
        player_id,
        gaps_started as u64,
        GAP_MIN_INTERVAL,
        GAP_INTERVAL_SPREAD,
    )
}

impl Player {
    fn new<R: rand::Rng + ?Sized>(
        rng: &mut R,
        player_id: PlayerId,
        seed: u64,
        config: &AchtungConfig,
    ) -> Self {
        let initial_size = 3.0;
        Self {
            is_alive: true,
            head: Blob {
                id: 0,
                size: initial_size,
                position: Position {
                    x: rand::distr::Uniform::new(0.0, config.arena_width as f32)
                        .unwrap()
                        .sample(rng),
                    y: rand::distr::Uniform::new(0.0, config.arena_height as f32)
                        .unwrap()
                        .sample(rng),
                },
            },
            body: vec![],
            direction: Angle {
                radians: rand::distr::Uniform::new(0.0, 2.0 * std::f32::consts::PI)
                    .unwrap()
                    .sample(rng),
            },
            speed: 2.0,
            turning_speed: 0.1,
            size: initial_size,
            action: GameAction::Forward,
            ticks_until_gap: initial_gap_delay(seed, player_id),
            gap_remaining: 0,
            gaps_started: 0,
        }
    }

    /// Advance the gap countdown by one tick. Returns `true` when the trail
    /// should be drawn this tick. Semantics match the reference frontend
    /// (`mockup/sim.js`): the countdown ticks down every tick (even mid-gap);
    /// when it hits zero a `GAP_LENGTH`-tick hole starts and a fresh
    /// interval is drawn.
    fn update_gap(&mut self, seed: u64, player_id: PlayerId) -> bool {
        if self.ticks_until_gap > 0 {
            self.ticks_until_gap -= 1;
        }
        if self.ticks_until_gap == 0 && self.gap_remaining == 0 {
            self.gap_remaining = GAP_LENGTH;
            self.gaps_started += 1;
            self.ticks_until_gap = next_gap_interval(seed, player_id, self.gaps_started);
        }
        if self.gap_remaining == 0 {
            true
        } else {
            self.gap_remaining -= 1;
            false
        }
    }

    fn diff(&self, other: &Player) -> PlayerDiff {
        // TODO: Make a macro for this
        PlayerDiff {
            is_alive: (self.is_alive != other.is_alive).then(|| self.is_alive),
            head: (self.head != other.head).then(|| self.head),
            body: self
                .body
                .iter()
                .filter(|b1| !other.body.iter().any(|b2| b1.id == b2.id))
                .copied()
                .collect(),
            direction: (self.direction != other.direction).then(|| self.direction),
            speed: (self.speed != other.speed).then(|| self.speed),
            turning_speed: (self.turning_speed != other.turning_speed).then(|| self.turning_speed),
            size: (self.size != other.size).then(|| self.size),
            action: (self.action != other.action).then(|| self.action),
            ticks_until_gap: (self.ticks_until_gap != other.ticks_until_gap)
                .then(|| self.ticks_until_gap),
            gap_remaining: (self.gap_remaining != other.gap_remaining).then(|| self.gap_remaining),
            gaps_started: (self.gaps_started != other.gaps_started).then(|| self.gaps_started),
        }
    }

    // Checks if player_1's head is colliding with player_2's body or own body
    fn collision(&self, player_2: &Player) -> bool {
        let head = &self.head;
        player_2.body.iter().any(|blob: &Blob| {
            let dx = head.position.x - blob.position.x;
            let dy = head.position.y - blob.position.y;
            let distance = (dx * dx + dy * dy).sqrt();
            distance < head.size + blob.size
        })
    }

    fn self_collision(&self) -> bool {
        let head = &self.head;
        self.body
            .iter()
            .rev()
            .skip(COLLISION_SELF_IGNORE_N_LATEST)
            .any(|blob: &Blob| {
                let dx = head.position.x - blob.position.x;
                let dy = head.position.y - blob.position.y;
                let distance = (dx * dx + dy * dy).sqrt();
                distance < head.size + blob.size
            })
    }

    fn wall_collision(&self, config: &AchtungConfig) -> bool {
        let head = &self.head;
        head.position.x < 0.0
            || head.position.x > config.arena_width as f32
            || head.position.y < 0.0
            || head.position.y > config.arena_height as f32
    }
}

impl game::GameState for Achtung {
    type PlayerId = PlayerId;
    type GameAction = GameAction;
    type StateDiff = AchtungDiff;
    type Config = AchtungConfig;

    fn init_game(config: &AchtungConfig, num_players: usize) -> Self {
        let seed: u64 = rand::rng().random();
        Self::init_game_with_seed(config, num_players, seed)
    }

    fn get_player_ids(&self) -> Vec<Self::PlayerId> {
        // Sorted = init order (players are created as 0..num_players): the
        // host binds slot `i` to `ids[i]`, so HashMap order here previously
        // mis-steered stateful agents. See `GameState::get_player_ids`.
        let mut ids: Vec<PlayerId> = self.players.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    fn diff(&self, other: &Achtung) -> AchtungDiff {
        AchtungDiff {
            timestep: self.timestep,
            players: self
                .players
                .iter()
                .map(|(&id, player)| (id, other.players.get(&id).unwrap().diff(&player)))
                .collect(),
        }
    }

    fn get_game_result(&self) -> Option<game::GameResult<PlayerId>> {
        match self
            .players
            .iter()
            .filter(|(_, p)| p.is_alive)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [(&winner_id, _)] => Some(game::GameResult::Winner(winner_id)),
            [] => Some(game::GameResult::NoWinner),
            _ => None,
        }
    }

    fn handle_player_action(&mut self, player_id: PlayerId, action: GameAction) {
        self.players
            .get_mut(&player_id)
            .expect("player should exist")
            .action = action;
    }

    fn handle_player_leave(&mut self, player_id: PlayerId) {
        self.kill_player(player_id);
    }

    fn update_game_state(&mut self) {
        self.timestep += 1;

        // Copy out what the per-player loop needs so we don't hold an
        // immutable borrow of `self` while mutating players.
        let seed = self.seed;
        let config = self.config.clone();
        // Update player positions
        for (id, player) in self.players.iter_mut().filter(|(_, p)| p.is_alive) {
            match player.action {
                GameAction::Left => player.direction.radians -= player.turning_speed,
                GameAction::Right => player.direction.radians += player.turning_speed,
                GameAction::Forward => {}
            }
            if player.update_gap(seed, *id) {
                player.body.push(player.head.clone());
            }
            let wrap = |x: f32, max: f32| (x % max + max) % max;
            let pos = match config.edge_wrapping {
                true => Position {
                    x: wrap(
                        player.head.position.x + player.direction.radians.cos() * player.speed,
                        config.arena_width as f32,
                    ),
                    y: wrap(
                        player.head.position.y + player.direction.radians.sin() * player.speed,
                        config.arena_height as f32,
                    ),
                },
                false => Position {
                    x: player.head.position.x + player.direction.radians.cos() * player.speed,
                    y: player.head.position.y + player.direction.radians.sin() * player.speed,
                },
            };
            player.head = Blob {
                id: player.head.id + 1,
                size: player.size,
                position: pos,
            };
        }
        let mut players_to_kill = HashSet::new();
        for (id1, p1) in self.players.iter().filter(|(_, p)| p.is_alive) {
            if p1.wall_collision(&self.config) || p1.self_collision() {
                players_to_kill.insert(*id1);
                continue;
            }
            for (id2, p2) in self.players.iter() {
                if id1 != id2 && p1.collision(p2) {
                    players_to_kill.insert(*id1);
                }
            }
        }
        for id in players_to_kill {
            self.kill_player(id);
        }
    }
}

/// Read-only view of a player's externally-observable state, for headless
/// drivers (e.g. the gRPC game host) that need to expose state to agents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerView {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    /// Heading in radians.
    pub direction: f32,
    pub alive: bool,
}

/// A single drawable circle (head or trail segment), for spectators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlobView {
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

/// Arena dimensions in game units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaSize {
    pub width: u32,
    pub height: u32,
}

impl Blob {
    fn to_view(self) -> BlobView {
        BlobView {
            x: self.position.x,
            y: self.position.y,
            size: self.size,
        }
    }
}

/// Full spectator view of a player: head plus the entire trail. Unlike
/// [`PlayerView`] (agent-facing, head only) this carries the body so the
/// browser can render the curve.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSpectatorView {
    pub player_id: PlayerId,
    pub alive: bool,
    pub head: BlobView,
    pub body: Vec<BlobView>,
}

impl Achtung {
    /// Current tick (timestep) counter.
    pub fn tick(&self) -> u64 {
        self.timestep
    }

    /// Deterministic constructor for the gap schedule: the same `seed`
    /// always yields the same gap timings per player. (Spawn positions and
    /// headings are still drawn from thread RNG.)
    pub fn init_game_with_seed(config: &AchtungConfig, num_players: usize, seed: u64) -> Self {
        let mut rng = rand::rng();
        Self {
            timestep: 0,
            players: (0..num_players)
                .into_iter()
                .map(|id| (id, Player::new(&mut rng, id, seed, config)))
                .collect(),
            config: config.clone(),
            seed,
        }
    }

    /// Arena dimensions.
    pub fn arena(&self) -> ArenaSize {
        ArenaSize {
            width: self.config.arena_width,
            height: self.config.arena_height,
        }
    }

    /// Full spectator snapshot: every player's head and trail, ordered by id.
    /// The body is append-only, so consumers can diff by trailing length.
    pub fn spectator_view(&self) -> Vec<PlayerSpectatorView> {
        let mut views: Vec<PlayerSpectatorView> = self
            .players
            .iter()
            .map(|(&player_id, p)| PlayerSpectatorView {
                player_id,
                alive: p.is_alive,
                head: p.head.to_view(),
                body: p.body.iter().map(|b| b.to_view()).collect(),
            })
            .collect();
        views.sort_by_key(|v| v.player_id);
        views
    }

    /// Snapshot of every player's observable state, ordered by player id.
    pub fn player_views(&self) -> Vec<PlayerView> {
        let mut views: Vec<PlayerView> = self
            .players
            .iter()
            .map(|(&player_id, p)| PlayerView {
                player_id,
                x: p.head.position.x,
                y: p.head.position.y,
                direction: p.direction.radians,
                alive: p.is_alive,
            })
            .collect();
        views.sort_by_key(|v| v.player_id);
        views
    }

    fn kill_player(&mut self, player_id: PlayerId) {
        log::info!("player {} died", player_id);
        self.players
            .get_mut(&player_id)
            .expect("player should exist")
            .is_alive = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameState as _;

    /// Slot binding depends on this order (the host maps slot `i` to
    /// `ids[i]`): ids must come back in init order every game, regardless of
    /// `HashMap` randomization. Guards the documented `get_player_ids`
    /// invariant — collecting `HashMap::keys()` directly failed this.
    #[test]
    fn player_ids_are_in_init_order_across_seeds() {
        for seed in [0, 1, 7, 42, 12345, 999_999] {
            let game = Achtung::init_game_with_seed(&AchtungConfig::default(), 8, seed);
            let ids = game.get_player_ids();
            assert_eq!(ids, (0..8).collect::<Vec<_>>(), "seed {seed}");
        }
    }

    /// Drive one player's gap scheduler and return the 1-indexed ticks on
    /// which gaps start plus the total number of non-drawing ticks.
    fn gap_starts(seed: u64, player_id: PlayerId, ticks: u32) -> Vec<u32> {
        let mut rng = rand::rng();
        let config = AchtungConfig::default();
        let mut player = Player::new(&mut rng, player_id, seed, &config);
        let mut starts = Vec::new();
        let mut in_gap = false;
        for tick in 1..=ticks {
            let draw = player.update_gap(seed, player_id);
            if !draw && !in_gap {
                starts.push(tick);
            }
            in_gap = !draw;
        }
        starts
    }

    #[test]
    fn first_gap_is_delayed_and_within_range() {
        let starts = gap_starts(12345, 0, 500);
        let first = starts[0];
        assert!(
            (INITIAL_GAP_DELAY_BASE..INITIAL_GAP_DELAY_BASE + INITIAL_GAP_DELAY_SPREAD)
                .contains(&first),
            "first gap at tick {first}, expected 40..160"
        );
    }

    #[test]
    fn gap_runs_have_fixed_length_and_random_intervals() {
        let seed = 999;
        let player_id = 0;
        let mut rng = rand::rng();
        let config = AchtungConfig::default();
        let mut player = Player::new(&mut rng, player_id, seed, &config);

        let mut run_length = 0u32;
        let mut runs = Vec::new();
        let mut starts = Vec::new();
        for tick in 1..=3000u32 {
            let draw = player.update_gap(seed, player_id);
            if !draw {
                if run_length == 0 {
                    starts.push(tick);
                }
                run_length += 1;
            } else if run_length > 0 {
                runs.push(run_length);
                run_length = 0;
            }
        }
        assert!(starts.len() >= 5, "expected several gaps in 3000 ticks");
        for run in &runs {
            assert_eq!(
                *run, GAP_LENGTH,
                "every gap must be exactly {GAP_LENGTH} ticks"
            );
        }
        let intervals: Vec<u32> = starts.windows(2).map(|w| w[1] - w[0]).collect();
        for interval in &intervals {
            assert!(
                (*interval >= GAP_MIN_INTERVAL)
                    && (*interval < GAP_MIN_INTERVAL + GAP_INTERVAL_SPREAD),
                "gap interval {interval} outside 120..320"
            );
        }
        // Randomized, not fixed-period: intervals must vary.
        assert!(
            intervals
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1,
            "gap intervals are constant ({intervals:?}); expected randomization"
        );
    }

    #[test]
    fn schedules_are_deterministic_and_player_specific() {
        let seed = 42;
        let a = gap_starts(seed, 0, 2000);
        let b = gap_starts(seed, 0, 2000);
        assert_eq!(a, b, "same seed + player must replay identically");

        let c = gap_starts(seed, 1, 2000);
        assert_ne!(a, c, "different players must have different schedules");

        let d = gap_starts(seed + 1, 0, 2000);
        assert_ne!(a, d, "different seeds must give different schedules");
    }

    #[test]
    fn players_spawn_with_desynchronized_gaps() {
        let game = Achtung::init_game_with_seed(&AchtungConfig::default(), 6, 7);
        let delays: Vec<u32> = (0..6)
            .map(|id| game.players.get(&id).unwrap().ticks_until_gap)
            .collect();
        for delay in &delays {
            assert!(
                (*delay >= INITIAL_GAP_DELAY_BASE)
                    && (*delay < INITIAL_GAP_DELAY_BASE + INITIAL_GAP_DELAY_SPREAD),
                "initial delay {delay} outside 40..160"
            );
        }
        assert!(
            delays
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1,
            "all players share initial delay {delays:?}; expected per-player desync"
        );
    }

    #[test]
    fn serde_round_trip_preserves_gap_schedule() {
        let seed = 2026;
        let game = Achtung::init_game_with_seed(&AchtungConfig::default(), 2, seed);
        let json = serde_json::to_string(&game).unwrap();
        let restored: Achtung = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.seed, seed);
        for id in 0..2 {
            let (orig, back) = (
                game.players.get(&id).unwrap(),
                restored.players.get(&id).unwrap(),
            );
            assert_eq!(orig.ticks_until_gap, back.ticks_until_gap);
            assert_eq!(orig.gap_remaining, back.gap_remaining);
            assert_eq!(orig.gaps_started, back.gaps_started);
            // Schedules must continue identically after the round-trip.
            let mut o = orig.clone();
            let mut r = back.clone();
            for _ in 0..500 {
                assert_eq!(
                    o.update_gap(seed, id),
                    r.update_gap(seed, id),
                    "gap schedule diverged after serde round-trip (player {id})"
                );
            }
        }
    }

    #[test]
    fn game_steps_draw_gaps_for_different_players_at_different_times() {
        // Huge wrapping arena with hand-placed parallel starts: nobody can
        // meet a wall or another trail within 60 straight ticks, so body
        // growth purely reflects the gap schedule.
        let config = AchtungConfig {
            arena_width: 10000,
            arena_height: 10000,
            edge_wrapping: true,
        };
        let mut game = Achtung::init_game_with_seed(&config, 4, 11);
        for id in 0..4 {
            let player = game.players.get_mut(&id).unwrap();
            player.head.position = Position {
                x: 1000.0 + id as f32 * 2000.0,
                y: 5000.0,
            };
            player.direction = Angle { radians: 0.0 };
        }
        // Record each player's per-tick draw pattern; 200 ticks guarantees
        // every player gaps at least once (max initial delay is 160).
        let mut patterns: Vec<Vec<bool>> = vec![Vec::new(); 4];
        for _ in 0..200 {
            let before: Vec<usize> = (0..4)
                .map(|id| game.players.get(&id).unwrap().body.len())
                .collect();
            game.update_game_state();
            for id in 0..4 {
                patterns[id].push(game.players.get(&id).unwrap().body.len() > before[id]);
            }
        }
        assert!(game.players.values().all(|p| p.is_alive));
        // Every player drew *something* but also gapped at least once.
        for (id, pattern) in patterns.iter().enumerate() {
            assert!(
                pattern.contains(&true) && pattern.contains(&false),
                "player {id} never gapped (or never drew) in 200 ticks"
            );
        }
        assert!(
            patterns
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1,
            "all players share one draw pattern; gaps look synchronized"
        );
    }
}
