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

type ClientId = usize;

/// Our global unique client id counter.
static NEXT_CLIENT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy)]
enum ClientType {
    Player,
    Observer,
}

impl std::str::FromStr for ClientType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "player" => Ok(Self::Player),
            "observer" => Ok(Self::Observer),
            _ => Err(format!("invalid client type: {}", s)),
        }
    }
}

#[derive(Clone)]
pub struct GameServer<const N: usize, T: game::GameState<N>> {
    game_config: T::Config,
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

#[derive(Default, Debug, Eq, PartialEq)]
enum GameSessionStatus<const N: usize, T: game::GameState<N>> {
    #[default]
    WaitingForPlayers,
    InProgress(T),
    GameOver,
}

struct GameSession<const N: usize, T: game::GameState<N>> {
    oberserver_channels: HashMap<ClientId, tokio::sync::mpsc::UnboundedSender<ws::Message>>,
    player_channels: HashMap<ClientId, tokio::sync::mpsc::UnboundedSender<ws::Message>>,
    player_ids: HashMap<ClientId, T::PlayerId>,
    game_status: GameSessionStatus<N, T>,
}

impl<const N: usize, T: game::GameState<N>> Default for GameSession<N, T> {
    fn default() -> Self {
        Self {
            oberserver_channels: HashMap::new(),
            player_channels: HashMap::new(),
            player_ids: HashMap::new(),
            game_status: GameSessionStatus::WaitingForPlayers,
        }
    }
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
        self.oberserver_channels.clear();
        self.game_status = GameSessionStatus::WaitingForPlayers;
    }

    fn broadcast_event(&self, event: GameEvent<N, T>) {
        let message = ws::Message::text(serde_json::to_string(&Event { event }).unwrap());
        for channel in self
            .player_channels
            .values()
            .chain(self.oberserver_channels.values())
        {
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

impl<const N: usize, T: game::GameState<N>> GameServer<N, T> {
    pub fn new(game_config: T::Config) -> Self {
        Self {
            game_config,
            lock: Arc::new(tokio::sync::RwLock::new(GameSession::default())),
        }
    }
}

impl<const N: usize, T> GameServer<N, T>
where
    T: game::GameState<N> + Serialize + Send + Sync + Clone + 'static,
    T::PlayerId: std::hash::Hash + std::fmt::Debug + Copy,
    T::PlayerId: Serialize + Send + Sync,
    T::StateDiff: Serialize + Send,
    T::GameAction: Serialize + DeserializeOwned + Send,
    T::Config: Clone + Send + Sync,
{
    pub async fn host_game(self) {
        pretty_env_logger::init();

        let index = warp::path::end().and(warp::fs::file("www/static/index.html"));
        let ws_routes = warp::path!("join" / ClientType)
            .and(warp::path::end())
            .and(warp::ws())
            .and(warp::any().map(move || self.clone()))
            .map(|client_type: ClientType, ws: warp::ws::Ws, server: Self| {
                ws.on_upgrade(move |socket| server.client_connected(client_type, socket))
            });

        warp::serve(index.or(ws_routes))
            .run(([127, 0, 0, 1], 3030))
            .await;
    }

    async fn client_connected(mut self, client_type: ClientType, ws: ws::WebSocket) {
        let mut game_session = self.lock.write().await;

        match (&game_session.game_status, &client_type) {
            (GameSessionStatus::InProgress(_), ClientType::Player) => {
                log::warn!("client tried to join a game that is in progress");
                ws.close().await.unwrap();
                return;
            }
            (GameSessionStatus::GameOver, _) => {
                log::warn!("client tried to connect to a game that is over");
                ws.close().await.unwrap();
                return;
            }
            _ => {}
        }

        let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
        log::info!("Client connected: {}", client_id);

        let (mut client_ws_tx, mut client_ws_rx) = ws.split();
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

        let channel = match client_type {
            ClientType::Player => &mut game_session.player_channels,
            ClientType::Observer => &mut game_session.oberserver_channels,
        };
        channel.insert(client_id, internal_tx);

        match client_type {
            ClientType::Player => {
                if game_session.player_channels.len() == N {
                    log::info!("All players connected, starting game");
                    self.start_game(&mut game_session).await;
                }
            }
            ClientType::Observer => {}
        }

        let _ = game_session.downgrade();

        while let Some(result) = client_ws_rx.next().await {
            match result {
                Ok(msg) if msg.is_close() => break,
                Ok(msg) if msg.is_text() => match client_type {
                    ClientType::Player => self.handle_message(client_id, msg).await,
                    ClientType::Observer => {}
                },
                Ok(_) => {}
                Err(error) => {
                    log::error!("websocket error(client={}): {}", client_id, error);
                    break;
                }
            }
        }

        match client_type {
            ClientType::Player => self.player_disconnected(client_id).await,
            ClientType::Observer => self.observer_disconnected(client_id).await,
        }
    }

    async fn start_game(&self, game_session: &mut GameSession<N, T>) {
        let game_state = T::init_game(&self.game_config);
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

    async fn game_loop(self, tick_interval: tokio::time::Duration) {
        loop {
            let mut game_session = self.lock.write().await;
            let game_state = match game_session.get_game_state() {
                Some(game_state) => game_state,
                None => {
                    log::info!("game ended, stopping game loop");
                    return;
                }
            };
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

    async fn observer_disconnected(&mut self, client_id: ClientId) {
        log::info!("observer disconnect: {}", client_id);
        let mut game_session = self.lock.write().await;
        game_session.oberserver_channels.remove(&client_id);
    }

    async fn player_disconnected(&mut self, client_id: ClientId) {
        log::info!("gamer disconnect: {}", client_id);
        let mut game_session = self.lock.write().await;
        game_session.player_channels.remove(&client_id);
        match game_session.player_ids.get(&client_id) {
            Some(&player_id) => {
                game_session
                    .get_game_state()
                    .map(|game_state| game_state.handle_player_leave(player_id));
            }
            None => {}
        };
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
