use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use rand::prelude::Distribution;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::Filter;

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

const GAME_WIDTH: f32 = 500.0;
const GAME_HEIGHT: f32 = 500.0;

type PlayerId = usize;
#[derive(Debug, Clone, Serialize, Deserialize)]
enum GameAction {
    Left,
    Right,
    Forward,
    // More like use item, etc.
}

#[derive(Serialize, Deserialize)]
enum EventMessage {
    PlayerEvent(PlayerEvent),
    GameEvent(GameEvent),
}

#[derive(Serialize, Deserialize)]
enum PlayerEvent {
    Join,
    Leave,
    Action(GameAction),
}

#[derive(Serialize, Deserialize)]
enum GameEvent {
    UpdateState(GameState),
    PlayerDied(PlayerId),
    PlayerJoined(PlayerId),
    GameOver { winner: Option<PlayerId> },
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
struct GameState {
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

#[derive(Default, PartialEq, Eq)]
enum GameSessionStatus {
    #[default]
    WaitingForPlayers,
    InProgress,
    GameOver,
}

#[derive(Default)]
struct GameSession {
    status: GameSessionStatus,
    player_channels: HashMap<PlayerId, mpsc::UnboundedSender<Message>>,
    game_state: GameState,
}
type StateLock = Arc<RwLock<GameSession>>;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Keep track of all connected users, key is usize, value
    // is a websocket sender.
    let state_lock = StateLock::default();

    // GET /game -> websocket upgrade
    let game = warp::path("game")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(warp::any().map(move || state_lock.clone()))
        .map(|ws: warp::ws::Ws, users| {
            // This will call our function if the handshake succeeds.
            ws.on_upgrade(move |socket| player_connected(socket, users))
        });

    // GET / -> index html
    let index = warp::path::end().map(|| warp::reply::html(INDEX_HTML));

    let routes = index.or(game);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

fn broadcast_message(message: Message, session: &GameSession) {
    for channel in session.player_channels.values() {
        channel.send(message.clone()).unwrap();
    }
}

async fn player_connected(ws: WebSocket, state_lock: StateLock) {
    if state_lock.read().await.status != GameSessionStatus::WaitingForPlayers {
        log::warn!("player tried to connect to a game that is not waiting for players");
        ws.close().await.unwrap();
        return;
    }

    // Use a counter to assign a new unique ID for this user.
    let player_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    log::info!("gamer connected: {}", player_id);

    // Split the socket into a sender and receiver of messages.
    let (mut client_ws_tx, mut client_ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (internal_tx, internal_rx) = mpsc::unbounded_channel();
    let mut internal_rx = UnboundedReceiverStream::new(internal_rx);

    tokio::task::spawn(async move {
        while let Some(message) = internal_rx.next().await {
            client_ws_tx
                .send(message)
                .unwrap_or_else(|e| {
                    log::warn!("websocket send error: {}", e);
                })
                .await;
        }
    });

    // Save the sender in our list of connected users.
    let mut game_session = state_lock.write().await;
    game_session.player_channels.insert(player_id, internal_tx);

    log::info!("players connected: {}", game_session.player_channels.len());

    // Start the game once we have enough players
    if game_session.downgrade().player_channels.len() >= 2 {
        log::info!("All players connected, starting game");
        start_game(state_lock.clone()).await;
    }

    // Return a `Future` that is basically a state machine managing
    // this specific players connection.
    while let Some(result) = client_ws_rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("websocket error(uid={}): {}", player_id, e);
                break;
            }
        };
        handle_message(player_id, msg, &state_lock).await;
    }

    // the above stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    player_disconnected(player_id, &state_lock).await;
}

async fn player_disconnected(player_id: PlayerId, state_lock: &StateLock) {
    eprintln!("gamer disconnect: {}", player_id);

    let mut game_session = state_lock.write().await;
    game_session.player_channels.remove(&player_id);
    kill_player(player_id, &mut game_session.game_state);
    let game_session = game_session.downgrade();

    // Send a message to all players that the player has left
    broadcast_message(
        Message::text(serde_json::to_string(&GameEvent::PlayerDied(player_id)).unwrap()),
        &game_session,
    );
}

async fn handle_message(player_id: PlayerId, msg: Message, state_lock: &StateLock) {
    if msg.is_close() {
        return;
    }
    let msg_text = match msg.to_str() {
        Ok(s) => s,
        Err(_) => {
            log::warn!("received non-text message from uid: {}", player_id);
            return;
        }
    };

    log::info!("received message from uid {}: {}", player_id, msg_text);

    let event: PlayerEvent = match serde_json::from_str(msg_text) {
        Ok(event) => event,
        Err(error) => {
            log::warn!(
                "error in parsing event {} from uid {}: {}",
                msg_text,
                player_id,
                error
            );
            return;
        }
    };

    handle_player_event(player_id, event, state_lock).await
}

async fn handle_player_event(my_id: PlayerId, player_event: PlayerEvent, state_lock: &StateLock) {
    match player_event {
        PlayerEvent::Action(action) => {
            // Update the player's action
            state_lock
                .write()
                .await
                .game_state
                .players
                .get_mut(&my_id)
                .expect("player should exist")
                .action = action;
        }
        PlayerEvent::Join => {
            let game_session = state_lock.read().await;
            broadcast_message(
                Message::text(serde_json::to_string(&GameEvent::PlayerJoined(my_id)).unwrap()),
                &game_session,
            );
        }
        PlayerEvent::Leave => {
            let mut game_session = state_lock.write().await;
            game_session.player_channels.remove(&my_id);
            let game_session = game_session.downgrade();
            broadcast_message(
                Message::text(serde_json::to_string(&GameEvent::PlayerDied(my_id)).unwrap()),
                &game_session,
            );
        }
    }
}

async fn start_game(state_lock: StateLock) {
    let mut game_session = state_lock.write().await;
    game_session.status = GameSessionStatus::InProgress;

    let mut rng = rand::thread_rng();

    // Spawn players
    game_session.game_state.players = game_session
        .player_channels
        .keys()
        .map(|id| (*id, Player::new(&mut rng)))
        .collect();

    let tick_interval = tokio::time::Duration::from_millis(16);
    tokio::task::spawn(game_loop(state_lock.clone(), tick_interval));
}

async fn game_loop(state_lock: StateLock, tick_interval: tokio::time::Duration) {
    loop {
        let mut game_session = state_lock.write().await;
        update_game_state(&mut game_session.game_state);
        match game_session
            .game_state
            .players
            .iter()
            .filter(|(_, p)| p.is_alive)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [&winner_id] => {
                log::info!("game over, winner: {}", winner_id);
                game_session.status = GameSessionStatus::GameOver;
                let game_session = game_session.downgrade();
                broadcast_message(
                    Message::text(
                        serde_json::to_string(&GameEvent::GameOver {
                            winner: Some(winner_id),
                        })
                        .unwrap(),
                    ),
                    &game_session,
                );
                return;
            }
            [] => {
                log::info!("game over, no winner");
                game_session.status = GameSessionStatus::GameOver;
                let game_session = game_session.downgrade();
                broadcast_message(
                    Message::text(
                        serde_json::to_string(&GameEvent::GameOver { winner: None }).unwrap(),
                    ),
                    &game_session,
                );
                return;
            }
            _ => {}
        }
        let game_session = game_session.downgrade();

        // Send the updated game state to all players
        let mut state_diff = game_session.game_state.clone();
        // state_diff.players.values_mut().for_each(|p| {
        //     *p = p.without_tail();
        // });
        broadcast_message(
            Message::text(serde_json::to_string(&GameEvent::UpdateState(state_diff)).unwrap()),
            &game_session,
        );

        tokio::time::sleep(tick_interval).await;
    }
}

fn update_game_state(game_state: &mut GameState) {
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
    // for player_id in players_to_kill {
    //     kill_player(player_id, game_state);
    // }
}

fn kill_player(player_id: PlayerId, game_state: &mut GameState) {
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

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <title>Giv akt, kurven!</title>
    </head>
    <body>
        <canvas id="game" width="500" height="500"></canvas>
        <script type="text/javascript">
        const canvas = document.getElementById('game');
        const ctx = canvas.getContext('2d');

        
        const uri = 'ws://' + location.host + '/game';
        const ws = new WebSocket(uri);
        ws.onopen = function() { console.log('Connected'); };
        ws.onclose = function() { console.log('Disconnected'); };
        
        function handleEvent(event) {
            // console.log(event);
            
            ctx.clearRect(0, 0, canvas.width, canvas.height);

            // dark blue background
            ctx.fillStyle = '#000033';
            ctx.fillRect(0, 0, canvas.width, canvas.height);
            
            for (const player of Object.values(event.UpdateState.players)) {
                if (!player.is_alive) {
                    continue;
                }

                for (const blob of player.body) {
                    ctx.fillStyle = '#00ccff';
                    ctx.beginPath();
                    ctx.arc(blob.position.x, blob.position.y, blob.size, 0, 2 * Math.PI);
                    ctx.fill();
                }

                // Give head different color
                head = player.head;
                ctx.fillStyle = '#ffcc00';
                ctx.beginPath();
                ctx.arc(head.position.x, head.position.y, head.size, 0, 2 * Math.PI);
                ctx.fill();
            }
            
        }

        ws.onmessage = function(msg) {
            handleEvent(JSON.parse(msg.data));
        };

        function sendAction(ws, action) {
            ws.send('{"Action": "' + action + '"}');
        }

        document.addEventListener('keydown', function(event) {
            switch (event.key) {
                case 'a':
                    sendAction(ws, 'Left');
                    break;
                case 'd':
                    sendAction(ws, 'Right');
                    break;
                case 'w':
                    sendAction(ws, 'Forward');
                    break;
            }
        });
        </script>
    </body>
</html>
"#;
