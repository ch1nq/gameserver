pub enum GameResult<P> {
    Winner(P),
    NoWinner,
}

pub trait GameState<const N: usize> {
    type PlayerId;
    type GameAction;
    type StateDiff;
    type Config;

    fn init_game(config: &Self::Config) -> Self;
    fn get_player_ids(&self) -> [Self::PlayerId; N];
    fn update_game_state(&mut self);
    fn handle_player_action(&mut self, player_id: Self::PlayerId, action: Self::GameAction);
    fn handle_player_leave(&mut self, player_id: Self::PlayerId);
    fn get_game_result(&self) -> Option<GameResult<Self::PlayerId>>;
    fn diff(&self, other: &Self) -> Self::StateDiff;
}
