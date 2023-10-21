use rand::prelude::Distribution;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const GAME_WIDTH: f32 = 500.0;
const GAME_HEIGHT: f32 = 500.0;

pub type PlayerId = usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameAction {
    Left,
    Right,
    Forward,
    // More like use item, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Blob {
    size: f32,
    position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameState {
    timestep: u64,
    players: HashMap<PlayerId, Player>,
}

impl GameState {
    pub fn diff(&self) -> GameState {
        let mut diff = self.clone();
        diff.players = diff
            .players
            .into_iter()
            .map(|(id, player)| (id, player.without_tail()))
            .collect();
        diff
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    skip_frequency: u32,
    skip_duration: u32,
}

impl Player {
    fn new<R: rand::Rng + ?Sized>(rng: &mut R) -> Self {
        let initial_size = 3.0;
        Self {
            is_alive: true,
            head: Blob {
                size: initial_size,
                position: Position {
                    x: rand::distributions::Uniform::new(0.0, GAME_WIDTH).sample(rng),
                    y: rand::distributions::Uniform::new(0.0, GAME_HEIGHT).sample(rng),
                },
            },
            body: vec![],
            direction: Angle {
                radians: rand::distributions::Uniform::new(0.0, 2.0 * std::f32::consts::PI)
                    .sample(rng),
            },
            speed: 2.0,
            turning_speed: 0.1,
            size: initial_size,
            action: GameAction::Forward,
            skip_frequency: 50,
            skip_duration: 15,
        }
    }

    fn without_tail(&self) -> Player {
        let mut new_player = self.clone();
        new_player.body.clear();
        new_player
    }
}

pub fn init_game(game_state: &mut GameState, player_ids: impl IntoIterator<Item = PlayerId>) {
    let mut rng = rand::thread_rng();

    // Spawn players
    game_state.players = player_ids
        .into_iter()
        .map(|id| (id, Player::new(&mut rng)))
        .collect();
}

pub enum GameResult {
    Winner(PlayerId),
    NoWinner,
}

pub fn get_game_result(game_state: &GameState) -> Option<GameResult> {
    match game_state
        .players
        .iter()
        .filter(|(_, p)| p.is_alive)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [(&winner_id, _)] => Some(GameResult::Winner(winner_id)),
        [] => Some(GameResult::NoWinner),
        _ => None,
    }
}

pub fn handle_player_action(game_state: &mut GameState, player_id: PlayerId, action: GameAction) {
    game_state
        .players
        .get_mut(&player_id)
        .expect("player should exist")
        .action = action;
}

pub fn handle_player_leave(game_state: &mut GameState, player_id: PlayerId) {
    kill_player(game_state, player_id);
}

pub fn update_game_state(game_state: &mut GameState) {
    game_state.timestep += 1;

    // Update player positions
    for player in game_state.players.values_mut().filter(|p| p.is_alive) {
        match player.action {
            GameAction::Left => player.direction.radians -= player.turning_speed,
            GameAction::Right => player.direction.radians += player.turning_speed,
            GameAction::Forward => {}
        }
        if game_state.timestep as u32 % player.skip_frequency > player.skip_duration {
            player.body.push(player.head.clone());
        }
        let wrap = |x: f32, max: f32| (x % max + max) % max;
        player.head = Blob {
            size: player.size,
            position: Position {
                x: wrap(
                    player.head.position.x + player.direction.radians.cos() * player.speed,
                    GAME_WIDTH,
                ),
                y: wrap(
                    player.head.position.y + player.direction.radians.sin() * player.speed,
                    GAME_HEIGHT,
                ),
            },
        };
    }
    // Check for collisions
    let players_to_kill = game_state
        .players
        .iter()
        .flat_map(|(id1, p1)| {
            game_state
                .players
                .iter()
                .map(move |(id2, p2)| ((*id1, p1), (*id2, p2)))
        })
        .filter(|((_, p1), (_, p2))| p1.is_alive && p2.is_alive)
        .map(|((id1, p1), (id2, p2))| {
            if id1 == id2 {
                (id1, self_collision(p1))
            } else {
                (id1, collision(p1, p2))
            }
        })
        .filter_map(|(id, col)| match col {
            true => Some(id),
            false => None,
        })
        .collect::<Vec<_>>();
    for player_id in players_to_kill {
        kill_player(game_state, player_id);
    }
}

fn kill_player(game_state: &mut GameState, player_id: PlayerId) {
    log::info!("player {} died", player_id);
    game_state
        .players
        .get_mut(&player_id)
        .expect("player should exist")
        .is_alive = false;
}

const COLLISION_SELF_IGNORE_N_LATEST: usize = 10;

// Checks if player_1's head is colliding with player_2's body or own body
fn collision(player_1: &Player, player_2: &Player) -> bool {
    let head = &player_1.head;
    player_2.body.iter().any(|blob: &Blob| {
        let dx = head.position.x - blob.position.x;
        let dy = head.position.y - blob.position.y;
        let distance = (dx * dx + dy * dy).sqrt();
        distance < head.size + blob.size
    })
}

fn self_collision(player: &Player) -> bool {
    let head = &player.head;
    player
        .body
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
