use crate::game;

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use warp::filters::ws::Message;
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
pub struct GameServer<T: game::GameState> {
    tick_interval: Option<tokio::time::Duration>,
    game_config: T::Config,
    lock: Arc<tokio::sync::RwLock<GameSession<T>>>,
    num_players: usize,
}

#[derive(Deserialize)]
#[serde(tag = "e")]
enum PlayerEvent<T: game::GameState> {
    Action { action: T::GameAction },
    RequestUpdate,
}

#[derive(Serialize)]
#[serde(tag = "e")]
enum GameEvent<T>
where
    T: game::GameState + Serialize,
    T::PlayerId: Serialize,
    T::StateDiff: Serialize,
{
    AssignPlayerId { player_id: T::PlayerId },
    InitialState { state: T },
    UpdateState { diff: T::StateDiff },
    GameOver { winner: Option<T::PlayerId> },
}

#[derive(Serialize)]
struct Event<T>
where
    T: game::GameState + Serialize,
    T::GameAction: Serialize,
    T::StateDiff: Serialize,
    T::PlayerId: Serialize,
{
    event: GameEvent<T>,
}

#[derive(Default, Debug, Eq, PartialEq)]
enum GameSessionStatus<T: game::GameState> {
    #[default]
    WaitingForPlayers,
    InProgress(T),
    GameOver,
}

struct GameSession<T: game::GameState> {
    oberserver_channels: HashMap<ClientId, tokio::sync::mpsc::UnboundedSender<ws::Message>>,
    player_channels: HashMap<ClientId, tokio::sync::mpsc::UnboundedSender<ws::Message>>,
    player_ids: HashMap<ClientId, T::PlayerId>,
    game_status: GameSessionStatus<T>,
}

impl<T: game::GameState> Default for GameSession<T> {
    fn default() -> Self {
        Self {
            oberserver_channels: HashMap::new(),
            player_channels: HashMap::new(),
            player_ids: HashMap::new(),
            game_status: GameSessionStatus::WaitingForPlayers,
        }
    }
}

fn encode_message<T: Serialize>(message: &T) -> ws::Message {
    ws::Message::binary(serde_json::to_string(message).unwrap().as_bytes())
}

fn decode_message<T: DeserializeOwned>(message: ws::Message) -> serde_json::Result<T> {
    serde_json::from_slice(&message.as_bytes())
}

impl<T> GameSession<T>
where
    T: Serialize + Clone,
    T: game::GameState,
    T::PlayerId: Serialize + std::fmt::Debug + Copy,
    T::StateDiff: Serialize,
    T::GameAction: Serialize,
{
    fn reset(&mut self) {
        log::info!("resetting game");
        self.player_channels
            .values()
            // .chain(self.oberserver_channels.values())
            .for_each(|channel| channel.send(ws::Message::close()).unwrap());
        self.player_channels.clear();
        // self.oberserver_channels.clear();
        self.game_status = GameSessionStatus::WaitingForPlayers;
    }

    fn broadcast_event(&self, event: GameEvent<T>) {
        let message = encode_message(&Event { event });
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

    fn update_game_state(&mut self) -> Option<game::GameResult<T::PlayerId>> {
        let game_state = match self.get_game_state() {
            Some(game_state) => game_state,
            None => {
                log::warn!("game ended, cannot update game state");
                return None;
            }
        };
        let old_game_state = game_state.clone();

        // Update the game state
        game_state.update_game_state();

        // Check if the game is over
        if let Some(result) = game_state.get_game_result() {
            self.handle_game_over(&result);
            Some(result)
        } else {
            // Send the updated game state to all players
            let diff = old_game_state.diff(&game_state);
            self.broadcast_event(GameEvent::UpdateState { diff });
            None
        }
    }

    fn handle_game_over(&mut self, result: &game::GameResult<T::PlayerId>) {
        self.game_status = GameSessionStatus::GameOver;
        let winner = match &result {
            game::GameResult::Winner(player_id) => Some(player_id),
            game::GameResult::NoWinner => None,
        };
        log::info!("game over, winner: {:?}", winner);
        self.broadcast_event(GameEvent::GameOver {
            winner: winner.copied(),
        });
        self.reset();
    }
}

impl<T: game::GameState> GameServer<T> {
    pub fn new(
        tick_interval: Option<tokio::time::Duration>,
        game_config: T::Config,
        num_players: usize,
    ) -> Self {
        Self {
            tick_interval,
            game_config,
            num_players,
            lock: Arc::new(tokio::sync::RwLock::new(GameSession::default())),
        }
    }
}

impl<T> GameServer<T>
where
    T: game::GameState + Serialize + Send + Sync + Clone + 'static,
    T::PlayerId: std::hash::Hash + std::fmt::Debug + Copy,
    T::PlayerId: Serialize + Send + Sync,
    T::StateDiff: Serialize + Send,
    T::GameAction: Serialize + DeserializeOwned + Send,
    T::Config: Clone + Send + Sync,
{
    pub async fn host_game(self, port: u16) {
        pretty_env_logger::init();

        let ws_routes = warp::path!("join" / ClientType)
            .and(warp::path::end())
            .and(warp::ws())
            .and(warp::any().map(move || self.clone()))
            .map(|client_type: ClientType, ws: warp::ws::Ws, server: Self| {
                ws.on_upgrade(move |socket| server.client_connected(client_type, socket))
            });

        warp::serve(ws_routes).run(([0, 0, 0, 0], port)).await;
    }

    async fn client_connected(mut self, client_type: ClientType, ws: ws::WebSocket) {
        let mut game_session = self.lock.write().await;

        match (&game_session.game_status, &client_type) {
            (GameSessionStatus::InProgress(_), ClientType::Player) => {
                log::warn!("player tried to join a game that is in progress. closing connection");
                ws.close().await.unwrap();
                return;
            }
            (GameSessionStatus::GameOver, _) => {
                log::warn!("client tried to connect to a game that is over. closing connection");
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

        // Send the current state to any observers joining while the game is in progress
        match (client_type, game_session.get_game_state()) {
            (ClientType::Observer, Some(game_state)) => {
                let event = Event {
                    event: GameEvent::<T>::InitialState {
                        state: game_state.clone(),
                    },
                };
                internal_tx.send(encode_message(&event)).unwrap();
            }
            _ => {}
        }

        let channel = match client_type {
            ClientType::Player => &mut game_session.player_channels,
            ClientType::Observer => &mut game_session.oberserver_channels,
        };
        channel.insert(client_id, internal_tx);

        if matches!(client_type, ClientType::Player)
            && game_session.player_channels.len() == self.num_players
        {
            log::info!("All players connected, starting game");
            self.start_game(&mut game_session).await;
        }

        let _ = game_session.downgrade();

        while let Some(result) = client_ws_rx.next().await {
            match result {
                Ok(msg) if msg.is_close() => break,
                Ok(msg) if msg.is_binary() => match client_type {
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

    async fn start_game(&self, game_session: &mut GameSession<T>) {
        let game_state = T::init_game(&self.game_config, self.num_players);
        game_session.player_ids = game_session
            .player_channels
            .iter()
            .zip(game_state.get_player_ids().into_iter())
            .map(|((&client_id, channel), player_id)| {
                let message = encode_message(&Event {
                    event: GameEvent::<T>::AssignPlayerId { player_id },
                });
                channel.send(message).unwrap();
                (client_id, player_id)
            })
            .collect();
        game_session.broadcast_event(GameEvent::InitialState {
            state: game_state.clone(),
        });
        game_session.game_status = GameSessionStatus::InProgress(game_state);

        if let Some(tick_interval) = self.tick_interval {
            tokio::task::spawn(self.clone().game_loop(tick_interval));
        }
    }

    async fn game_loop(self, tick_interval: tokio::time::Duration) {
        loop {
            let mut game_session = self.lock.write().await;
            match game_session.game_status {
                GameSessionStatus::InProgress(_) => {
                    game_session.update_game_state();
                    drop(game_session);
                    tokio::time::sleep(tick_interval).await;
                }
                GameSessionStatus::GameOver | GameSessionStatus::WaitingForPlayers => {
                    log::info!("Game over, stopping tick requests");
                    break;
                }
            }
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
                if let Some(game_state) = game_session.get_game_state() {
                    game_state.handle_player_leave(player_id);
                    if let Some(result) = game_state.get_game_result() {
                        game_session.handle_game_over(&result);
                    }
                }
            }
            None => {}
        };
    }

    async fn handle_message(&mut self, client_id: ClientId, msg: Message) {
        let event: PlayerEvent<T> = match decode_message(msg) {
            Ok(event) => event,
            Err(error) => {
                log::warn!("error in parsing event {} from player {}", client_id, error);
                return;
            }
        };
        self.handle_player_event(client_id, event).await;
    }

    async fn handle_player_event(&mut self, client_id: ClientId, player_event: PlayerEvent<T>) {
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
            PlayerEvent::RequestUpdate if self.tick_interval.is_some() => log::warn!(
                "player {} requested tick not allowed when tick interval is set",
                client_id
            ),
            PlayerEvent::RequestUpdate => {
                log::debug!("player {} requested tick", client_id);
                game_session.update_game_state();
            }
        }
    }
}
