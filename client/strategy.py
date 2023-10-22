import abc
from concurrent.futures import Future, ThreadPoolExecutor

import game


class Strategy(abc.ABC):
    @abc.abstractmethod
    def take_action(self, game_state: game.GameState) -> game.GameAction | None:
        ...


class SlowStrategy(Strategy):
    """A strategy where each action takes longer to compute than the server's tick rate."""

    def __init__(self, strategy: Strategy):
        self.strategy = strategy
        self.action_job: Future[game.GameAction | None] | None = None

    def take_action(self, game_state: game.GameState) -> game.GameAction | None:
        if self.action_job is None:
            self.action_job = ThreadPoolExecutor().submit(self.strategy.take_action, game_state)
            return None
        elif self.action_job.done():
            action = self.action_job.result()
            self.action_job = None
            return action
        else:
            return None
