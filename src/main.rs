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

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Serialize, Deserialize)]
enum EventMessage {
    PlayerEvent(PlayerEvent),
    GameEvent(GameEvent),
}

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
type StateLock = Arc<RwLock<GameSession>>;

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

fn broadcast_message(message: Message, session: &GameSession) {
    for channel in session.player_channels.values() {
        channel.send(message.clone()).unwrap();
    }
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
    game_session.player_channels.insert(player_id, internal_tx);

    log::info!(
        "number of players connected: {}",
        game_session.player_channels.len()
    );

    // Start the game once we have enough players
    if game_session.player_channels.len() >= 2 {
        log::info!("All players connected, starting game");
        let mut game_state = game::GameState::default();
        let player_ids = game_session.player_channels.keys().copied();
        game::init_game(&mut game_state, player_ids);
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

fn reset(game_session: &mut GameSession) {
    log::info!("resetting game");
    game_session.player_channels.clear();
    game_session.game_status = GameSessionStatus::WaitingForPlayers;
}

async fn game_loop(state_lock: StateLock, tick_interval: tokio::time::Duration) {
    loop {
        let mut game_session = state_lock.write().await;
        let game_state = match &mut game_session.game_status {
            GameSessionStatus::InProgress(game_state) => game_state,
            _ => {
                log::error!("game loop called on game that is not in progress");
                return;
            }
        };

        game::update_game_state(game_state);
        match game::get_game_result(game_state) {
            Some(result) => {
                game_session.game_status = GameSessionStatus::GameOver;
                let winner = match result {
                    game::GameResult::Winner(player_id) => Some(player_id),
                    game::GameResult::NoWinner => None,
                };
                log::info!("game over, winner: {:?}", winner);
                broadcast_message(
                    Message::text(serde_json::to_string(&GameEvent::GameOver { winner }).unwrap()),
                    &game_session,
                );
                reset(&mut game_session);
                return;
            }
            None => {}
        }

        // Send the updated game state to all players
        broadcast_message(
            Message::text(
                serde_json::to_string(&GameEvent::UpdateState(game_state.diff())).unwrap(),
            ),
            &game_session,
        );

        // Wait for the next tick
        tokio::time::sleep(tick_interval).await;
    }
}

async fn player_disconnected(player_id: game::PlayerId, state_lock: &StateLock) {
    eprintln!("gamer disconnect: {}", player_id);

    let mut game_session = state_lock.write().await;
    game_session.player_channels.remove(&player_id);
    match &mut game_session.game_status {
        GameSessionStatus::InProgress(game_state) => {
            game::handle_player_leave(game_state, player_id);
        }
        _ => {}
    }
    let game_session = game_session.downgrade();

    // Send a message to all players that the player has left
    broadcast_message(
        Message::text(serde_json::to_string(&GameEvent::PlayerDied(player_id)).unwrap()),
        &game_session,
    );
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
        PlayerEvent::Action(action) => {
            if let GameSessionStatus::InProgress(game_state) =
                &mut state_lock.write().await.game_status
            {
                game::handle_player_action(game_state, player_id, action);
            } else {
                log::warn!("player tried to send action to game that is not in progress");
                return;
            }
        }
        PlayerEvent::Join => {
            let game_session = state_lock.read().await;
            broadcast_message(
                Message::text(serde_json::to_string(&GameEvent::PlayerJoined(player_id)).unwrap()),
                &game_session,
            );
        }
        PlayerEvent::Leave => {
            let mut game_session = state_lock.write().await;
            game_session.player_channels.remove(&player_id);
            let game_session = game_session.downgrade();
            broadcast_message(
                Message::text(serde_json::to_string(&GameEvent::PlayerDied(player_id)).unwrap()),
                &game_session,
            );
        }
    }
}
