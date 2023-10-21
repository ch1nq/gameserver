use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt, TryFutureExt};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::Filter;

mod game;

#[derive(Serialize, Deserialize)]
enum PlayerEvent {
    Join,
    Leave,
    Action(game::GameAction),
}

#[derive(Serialize, Deserialize)]
enum GameEvent {
    UpdateState(game::GameState),
    PlayerDied(game::PlayerId),
    PlayerJoined(game::PlayerId),
    GameOver { winner: Option<game::PlayerId> },
}

#[derive(Default, Debug)]
enum GameSessionStatus {
    #[default]
    WaitingForPlayers,
    InProgress(game::GameState),
    GameOver,
}

#[derive(Default)]
struct GameSession {
    player_channels: HashMap<game::PlayerId, mpsc::UnboundedSender<Message>>,
    game_status: GameSessionStatus,
}

impl GameSession {
    fn reset(&mut self) {
        log::info!("resetting game");
        self.player_channels.clear();
        self.game_status = GameSessionStatus::WaitingForPlayers;
    }

    fn broadcast_event(&self, event: GameEvent) {
        let message = Message::text(serde_json::to_string(&event).unwrap());
        for channel in self.player_channels.values() {
            channel.send(message.clone()).unwrap();
        }
    }

    fn get_game_state(&mut self) -> Option<&mut game::GameState> {
        match &mut self.game_status {
            GameSessionStatus::InProgress(game_state) => Some(game_state),
            _ => None,
        }
    }
}

type StateLock = Arc<RwLock<GameSession>>;

/// Our global unique player id counter.
static NEXT_PLAYER_ID: AtomicUsize = AtomicUsize::new(1);

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Initialize server state
    let state_lock = StateLock::default();

    let index = warp::path::end().and(warp::fs::file("www/static/index.html"));

    // GET /game -> websocket upgrade
    let game = warp::path("game")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(warp::any().map(move || state_lock.clone()))
        .map(|ws: warp::ws::Ws, state_lock| {
            // This will call our function if the handshake succeeds.
            ws.on_upgrade(move |socket| player_connected(socket, state_lock))
        });

    warp::serve(index.or(game))
        .run(([127, 0, 0, 1], 3030))
        .await;
}

async fn player_connected(ws: WebSocket, state_lock: StateLock) {
    let mut game_session = state_lock.write().await;

    match game_session.game_status {
        GameSessionStatus::WaitingForPlayers => {}
        _ => {
            log::warn!("player tried to connect to a game that is not waiting for players");
            ws.close().await.unwrap();
            return;
        }
    }

    // Use a counter to assign a new unique ID for this user.
    let player_id = NEXT_PLAYER_ID.fetch_add(1, Ordering::Relaxed);

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
    game_session.player_channels.insert(player_id, internal_tx);

    log::info!(
        "number of players connected: {}",
        game_session.player_channels.len()
    );

    // Start the game once we have enough players
    if game_session.player_channels.len() >= 4 {
        log::info!("All players connected, starting game");
        let mut game_state = game::GameState::default();
        let player_ids = game_session.player_channels.keys().copied();
        game_state.init_game(player_ids);
        game_session.game_status = GameSessionStatus::InProgress(game_state);

        let tick_interval = tokio::time::Duration::from_millis(16);
        tokio::task::spawn(game_loop(state_lock.clone(), tick_interval));
    }

    let _ = game_session.downgrade();

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

async fn game_loop(state_lock: StateLock, tick_interval: tokio::time::Duration) {
    loop {
        let mut game_session = state_lock.write().await;
        let game_state = game_session.get_game_state().expect("game state not found");
        let old_game_state = game_state.clone();

        // Update the game state
        game_state.update_game_state();

        // Check if the game is over
        match game_state.get_game_result() {
            Some(result) => {
                game_session.game_status = GameSessionStatus::GameOver;
                let winner = match result {
                    game::GameResult::Winner(player_id) => Some(player_id),
                    game::GameResult::NoWinner => None,
                };
                log::info!("game over, winner: {:?}", winner);
                game_session.broadcast_event(GameEvent::GameOver { winner });
                game_session.reset();
                return;
            }
            None => {}
        }

        // Send the updated game state to all players
        let diff = old_game_state.diff(&game_state);
        game_session.broadcast_event(GameEvent::UpdateState(diff));

        // Wait for the next tick
        tokio::time::sleep(tick_interval).await;
    }
}

async fn player_disconnected(player_id: game::PlayerId, state_lock: &StateLock) {
    eprintln!("gamer disconnect: {}", player_id);

    let mut game_session = state_lock.write().await;
    game_session.player_channels.remove(&player_id);
    game_session.get_game_state().map(|game_state| {
        game_state.handle_player_leave(player_id);
    });

    // Send a message to all players that the player has left
    game_session.broadcast_event(GameEvent::PlayerDied(player_id));
}

async fn handle_message(player_id: game::PlayerId, msg: Message, state_lock: &StateLock) {
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

async fn handle_player_event(
    player_id: game::PlayerId,
    player_event: PlayerEvent,
    state_lock: &StateLock,
) {
    match player_event {
        PlayerEvent::Action(action) => match state_lock.write().await.get_game_state() {
            Some(game_state) => game_state.handle_player_action(player_id, action),
            None => log::warn!("player tried to send action to game that is not in progress"),
        },
        PlayerEvent::Join => {
            state_lock
                .read()
                .await
                .broadcast_event(GameEvent::PlayerJoined(player_id));
        }
        PlayerEvent::Leave => {
            let mut game_session = state_lock.write().await;
            game_session.player_channels.remove(&player_id);
            game_session.broadcast_event(GameEvent::PlayerDied(player_id));
        }
    }
}
