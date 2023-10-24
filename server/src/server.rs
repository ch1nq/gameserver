use crate::game;

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use warp::ws;
use warp::Filter;

/// Our global unique player id counter.
static NEXT_PLAYER_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone)]
pub struct GameServer<const N: usize, T: game::GameState<N>> {
    lock: Arc<tokio::sync::RwLock<GameSession<N, T>>>,
}

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
    T: game::GameState<N> + Serialize,
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
    player_channels: HashMap<ClientId, tokio::sync::mpsc::UnboundedSender<ws::Message>>,
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
        let message = ws::Message::text(serde_json::to_string(&Event { event }).unwrap());
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

impl<const N: usize, T> GameServer<N, T>
where
    T: game::GameState<N> + Default,
    T::PlayerId: Default,
{
    pub fn new() -> Self {
        Self {
            lock: Default::default(),
        }
    }
}

impl<const N: usize, T> GameServer<N, T>
where
    T: game::GameState<N> + Default + Serialize + Send + Sync + Clone + 'static,
    T::PlayerId: std::hash::Hash + std::fmt::Debug + Copy,
    T::PlayerId: Serialize + Send + Sync,
    T::StateDiff: Serialize + Send,
    T::GameAction: Serialize + DeserializeOwned + Send,
{
    pub async fn host_game(self) {
        pretty_env_logger::init();

        let index = warp::path::end().and(warp::fs::file("www/static/index.html"));

        // GET /game -> websocket upgrade
        let game = warp::path("game")
            // The `ws()` filter will prepare Websocket handshake...
            .and(warp::ws())
            .and(warp::any().map(move || self.clone()))
            .map(|ws: warp::ws::Ws, state_lock: Self| {
                // This will call our function if the handshake succeeds.
                ws.on_upgrade(move |socket| state_lock.player_connected(socket))
            });

        warp::serve(index.or(game))
            .run(([127, 0, 0, 1], 3030))
            .await;
    }

    async fn player_connected(mut self, ws: ws::WebSocket) {
        let mut game_session = self.lock.write().await;

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
        let (internal_tx, internal_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut internal_rx = tokio_stream::wrappers::UnboundedReceiverStream::new(internal_rx);

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
                    let message = ws::Message::text(
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
            tokio::task::spawn(self.clone().game_loop(tick_interval));
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
            self.handle_message(client_id, msg).await;
        }

        // the above stream will keep processing as long as the user stays
        // connected. Once they disconnect, then...
        self.player_disconnected(client_id).await;
    }

    async fn game_loop(self, tick_interval: tokio::time::Duration) {
        loop {
            let mut game_session = self.lock.write().await;
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

    async fn player_disconnected(&mut self, client_id: ClientId) {
        log::info!("gamer disconnect: {}", client_id);
        let mut game_session = self.lock.write().await;
        game_session.player_channels.remove(&client_id);
        let player_id = *game_session
            .player_ids
            .get(&client_id)
            .expect("player id should exist");
        game_session.get_game_state().map(|game_state| {
            game_state.handle_player_leave(player_id);
        });
    }

    async fn handle_message(&mut self, client_id: ClientId, msg: ws::Message) {
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

        self.handle_player_event(client_id, event).await;
    }

    async fn handle_player_event(&mut self, client_id: ClientId, player_event: PlayerEvent<N, T>) {
        let mut game_session = self.lock.write().await;
        let player_id = *game_session
            .player_ids
            .get(&client_id)
            .expect("player id should exist");
        match player_event {
            PlayerEvent::Action { action } => match game_session.get_game_state() {
                Some(game_state) => game_state.handle_player_action(player_id, action),
                None => log::warn!("player tried to send action to game that is not in progress"),
            },
        }
    }
}
