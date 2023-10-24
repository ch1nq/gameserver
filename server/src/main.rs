use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt, TryFutureExt};

use gameserver::game;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::Filter;

#[derive(Deserialize)]
#[serde(tag = "event_type")]
enum PlayerEvent<const N: usize, T: game::GameState<N>> {
    Action { action: T::GameAction },
}

#[derive(Serialize)]
#[serde(tag = "event_type")]
enum GameEvent<const N: usize, T>
where
    T: game::GameState<N> + Serialize,
    T::PlayerId: Serialize,
    T::StateDiff: Serialize,
{
    AssignPlayerId { player_id: T::PlayerId },
    UpdateState { new_state: T::StateDiff },
    GameOver { winner: Option<T::PlayerId> },
}

#[derive(Serialize)]
struct Event<const N: usize, T>
where
    T: Serialize,
    T: game::GameState<N>,
    T::GameAction: Serialize,
    T::StateDiff: Serialize,
    T::PlayerId: Serialize,
{
    event: GameEvent<N, T>,
}

#[derive(Default, Debug)]
enum GameSessionStatus<const N: usize, T: game::GameState<N>> {
    #[default]
    WaitingForPlayers,
    InProgress(T),
    GameOver,
}

type ClientId = usize;

#[derive(Default)]
struct GameSession<const N: usize, T: game::GameState<N>> {
    player_channels: HashMap<ClientId, mpsc::UnboundedSender<Message>>,
    player_ids: HashMap<ClientId, T::PlayerId>,
    game_status: GameSessionStatus<N, T>,
}

impl<const N: usize, T> GameSession<N, T>
where
    T: Serialize,
    T: game::GameState<N>,
    T::PlayerId: Serialize,
    T::StateDiff: Serialize,
    T::GameAction: Serialize,
{
    fn reset(&mut self) {
        log::info!("resetting game");
        self.player_channels.clear();
        self.game_status = GameSessionStatus::WaitingForPlayers;
    }

    fn broadcast_event(&self, event: GameEvent<N, T>) {
        let message = Message::text(serde_json::to_string(&Event { event }).unwrap());
        for channel in self.player_channels.values() {
            channel.send(message.clone()).unwrap();
        }
    }

    fn get_game_state(&mut self) -> Option<&mut T> {
        match &mut self.game_status {
            GameSessionStatus::InProgress(game_state) => Some(game_state),
            _ => None,
        }
    }
}

type StateLock<const N: usize, T> = Arc<RwLock<GameSession<N, T>>>;

/// Our global unique player id counter.
static NEXT_PLAYER_ID: AtomicUsize = AtomicUsize::new(1);

#[tokio::main]
async fn main() {
    const NUMBER_OF_PLAYERS: usize = 4;
    host_game::<NUMBER_OF_PLAYERS, gameserver::games::achtung::Achtung>().await;
}

async fn host_game<const N: usize, G>()
where
    G: game::GameState<N> + Default + Serialize + Send + Sync + Clone + 'static,
    G::PlayerId: Hash + Eq + PartialEq + Serialize + Default + Send + Sync + std::fmt::Debug + Copy,
    G::StateDiff: Serialize + Send,
    G::GameAction: Serialize + DeserializeOwned + Send,
{
    pretty_env_logger::init();

    // Initialize server state
    let state_lock: StateLock<N, G> = Default::default();

    let index = warp::path::end().and(warp::fs::file("www/static/index.html"));

    // GET /game -> websocket upgrade
    let game = warp::path("game")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(warp::any().map(move || state_lock.clone()))
        .map(|ws: warp::ws::Ws, state_lock| {
            // This will call our function if the handshake succeeds.
            ws.on_upgrade(move |socket| player_connected::<N, G>(socket, state_lock))
        });

    warp::serve(index.or(game))
        .run(([127, 0, 0, 1], 3030))
        .await;
}

async fn player_connected<const N: usize, T>(ws: WebSocket, state_lock: StateLock<N, T>)
where
    T: game::GameState<N> + Default + Serialize + Send + Sync + Clone + 'static,
    T::PlayerId: Hash + Eq + PartialEq + std::fmt::Debug + Copy,
    T::PlayerId: Serialize + Send + Sync,
    T::StateDiff: Serialize + Send,
    T::GameAction: Serialize + DeserializeOwned + Send,
{
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
    let client_id = NEXT_PLAYER_ID.fetch_add(1, Ordering::Relaxed);

    log::info!("Client connected: {}", client_id);

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
    game_session.player_channels.insert(client_id, internal_tx);

    log::info!(
        "number of players connected: {}",
        game_session.player_channels.len()
    );

    // Start the game once we have enough players
    if game_session.player_channels.len() >= N {
        log::info!("All players connected, starting game");
        let mut game_state = T::default();
        game_state.init_game();
        game_session.player_ids = game_session
            .player_channels
            .iter()
            .zip(game_state.get_player_ids().into_iter())
            .map(|((&client_id, channel), player_id)| {
                let message = Message::text(
                    serde_json::to_string(&Event {
                        event: GameEvent::<N, T>::AssignPlayerId { player_id },
                    })
                    .unwrap(),
                );
                channel.send(message).unwrap();
                (client_id, player_id)
            })
            .collect();
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
                log::error!("websocket error(player={}): {}", client_id, e);
                break;
            }
        };
        if msg.is_close() {
            break;
        }
        let mut game_session = state_lock.write().await;
        game_session.handle_message(client_id, msg);
        let _ = game_session.downgrade();
    }

    // the above stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    let mut game_session = state_lock.write().await;
    game_session.player_disconnected(client_id);
    let _ = game_session.downgrade();
}

async fn game_loop<const N: usize, T>(
    state_lock: StateLock<N, T>,
    tick_interval: tokio::time::Duration,
) where
    T: game::GameState<N> + Serialize + Clone,
    T::PlayerId: Serialize + std::fmt::Debug,
    T::StateDiff: Serialize,
    T::GameAction: Serialize,
{
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
        game_session.broadcast_event(GameEvent::UpdateState { new_state: diff });

        // Wait for the next tick
        tokio::time::sleep(tick_interval).await;
    }
}

impl<const N: usize, T> GameSession<N, T>
where
    T: game::GameState<N> + Serialize,
    T::PlayerId: Serialize + Copy + std::fmt::Debug,
    T::StateDiff: Serialize,
    T::GameAction: Serialize + DeserializeOwned,
{
    fn player_disconnected(&mut self, client_id: ClientId) {
        log::info!("gamer disconnect: {}", client_id);

        self.player_channels.remove(&client_id);
        let player_id = *self
            .player_ids
            .get(&client_id)
            .expect("player id should exist");

        self.get_game_state().map(|game_state| {
            game_state.handle_player_leave(player_id);
        });
    }

    fn handle_message(&mut self, client_id: ClientId, msg: Message) {
        let msg_text = match msg.to_str() {
            Ok(s) => s,
            Err(_) => {
                log::warn!("received non-text message from player: {}", client_id);
                return;
            }
        };

        log::info!("received message from player {}: {}", client_id, msg_text);

        let event: PlayerEvent<N, T> = match serde_json::from_str(msg_text) {
            Ok(event) => event,
            Err(error) => {
                log::warn!(
                    "error in parsing event {} from player {}: {}",
                    msg_text,
                    client_id,
                    error
                );
                return;
            }
        };

        self.handle_player_event(client_id, event);
    }

    fn handle_player_event(&mut self, client_id: ClientId, player_event: PlayerEvent<N, T>) {
        log::info!("player ids: {:?}", self.player_ids);
        let player_id = *self
            .player_ids
            .get(&client_id)
            .expect("player id should exist");
        match player_event {
            PlayerEvent::Action { action } => match self.get_game_state() {
                Some(game_state) => game_state.handle_player_action(player_id, action),
                None => log::warn!("player tried to send action to game that is not in progress"),
            },
        }
    }
}
