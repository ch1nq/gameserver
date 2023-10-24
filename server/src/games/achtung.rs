use rand::prelude::Distribution;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::game;

const GAME_WIDTH: f32 = 500.0;
const GAME_HEIGHT: f32 = 500.0;

pub type PlayerId = usize;

#[derive(Serialize, Deserialize)]
pub enum GameEvent {
    UpdateState(Achtung),
    PlayerDied(PlayerId),
    PlayerJoined(PlayerId),
    GameOver { winner: Option<PlayerId> },
}

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

type BlobId = usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Blob {
    id: BlobId,
    size: f32,
    position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Achtung {
    timestep: u64,
    players: HashMap<PlayerId, Player>,
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

const COLLISION_SELF_IGNORE_N_LATEST: usize = 10;

impl Player {
    fn new<R: rand::Rng + ?Sized>(rng: &mut R) -> Self {
        let initial_size = 3.0;
        Self {
            is_alive: true,
            head: Blob {
                id: 0,
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

    fn with_tail_diff(&self, other: &Player) -> Self {
        let mut new_player = self.clone();
        new_player.body = new_player
            .body
            .into_iter()
            .filter(|b1| !other.body.iter().any(|b2| b1.id == b2.id))
            .collect();
        new_player
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
}

impl<const N: usize> game::GameState<N> for Achtung {
    type PlayerId = PlayerId;
    type GameAction = GameAction;
    type StateDiff = Achtung;

    fn init_game(&mut self) {
        let mut rng = rand::thread_rng();

        // Spawn players
        self.players = (0..N)
            .into_iter()
            .map(|id| (id, Player::new(&mut rng)))
            .collect();
    }

    fn get_player_ids(&self) -> [Self::PlayerId; N] {
        self.players
            .keys()
            .copied()
            .collect::<Vec<_>>()
            .try_into()
            .expect("should have N players")
    }

    fn diff(&self, other: &Achtung) -> Achtung {
        let mut diff = self.clone();
        diff.players = diff
            .players
            .into_iter()
            .map(|(id, player)| (id, other.players.get(&id).unwrap().with_tail_diff(&player)))
            .collect();
        diff
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

        // Update player positions
        for player in self.players.values_mut().filter(|p| p.is_alive) {
            match player.action {
                GameAction::Left => player.direction.radians -= player.turning_speed,
                GameAction::Right => player.direction.radians += player.turning_speed,
                GameAction::Forward => {}
            }
            if self.timestep as u32 % player.skip_frequency > player.skip_duration {
                player.body.push(player.head.clone());
            }
            let wrap = |x: f32, max: f32| (x % max + max) % max;
            player.head = Blob {
                id: player.head.id + 1,
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
        let players_to_kill = self
            .players
            .iter()
            .filter(|(_, p)| p.is_alive)
            .flat_map(|(id1, p1)| {
                self.players
                    .iter()
                    .map(move |(id2, p2)| ((*id1, p1), (*id2, p2)))
            })
            .map(|((id1, p1), (id2, p2))| {
                if id1 == id2 {
                    (id1, p1.self_collision())
                } else {
                    (id1, p1.collision(p2))
                }
            })
            .filter_map(|(id, col)| match col {
                true => Some(id),
                false => None,
            })
            .collect::<Vec<_>>();
        for player_id in players_to_kill {
            self.kill_player(player_id);
        }
    }
}

impl Achtung {
    fn kill_player(&mut self, player_id: PlayerId) {
        log::info!("player {} died", player_id);
        self.players
            .get_mut(&player_id)
            .expect("player should exist")
            .is_alive = false;
    }
}
