import abc
from concurrent.futures import Future, ThreadPoolExecutor

import achtung


class Strategy(abc.ABC):
    @abc.abstractmethod
    def take_action(self, game_state: achtung.GameState, player_id: achtung.PlayerId) -> achtung.GameAction | None:
        ...


class SlowStrategy(Strategy):
    """A strategy where each action takes longer to compute than the server's tick rate."""

    def __init__(self, strategy: Strategy):
        self.strategy = strategy
        self.action_job: Future[achtung.GameAction | None] | None = None

    def take_action(self, game_state: achtung.GameState, player_id: achtung.PlayerId) -> achtung.GameAction | None:
        if self.action_job is None:
            self.action_job = ThreadPoolExecutor().submit(self.strategy.take_action, game_state, player_id)
            return None
        elif self.action_job.done():
            action = self.action_job.result()
            self.action_job = None
            return action
        else:
            return None
