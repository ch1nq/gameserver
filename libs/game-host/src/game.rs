pub enum GameResult<P> {
    Winner(P),
    NoWinner,
}

pub trait GameState {
    type PlayerId;
    type GameAction;
    type StateDiff;
    type Config;

    fn init_game(config: &Self::Config, num_players: usize) -> Self;
    /// All player ids **in init order**: index `i` must be the *i*-th player
    /// created by [`init_game`](Self::init_game).
    ///
    /// The host binds match slot `i` to `get_player_ids()[i]`, so this order
    /// is load-bearing — collecting from an unordered map (e.g. `HashMap`)
    /// silently steers every stateful agent with someone else's observations.
    /// Keep it sorted/deterministic; the engine contract test pins this.
    fn get_player_ids(&self) -> Vec<Self::PlayerId>;
    fn update_game_state(&mut self);
    fn handle_player_action(&mut self, player_id: Self::PlayerId, action: Self::GameAction);
    fn handle_player_leave(&mut self, player_id: Self::PlayerId);
    fn get_game_result(&self) -> Option<GameResult<Self::PlayerId>>;
    fn diff(&self, other: &Self) -> Self::StateDiff;
}
