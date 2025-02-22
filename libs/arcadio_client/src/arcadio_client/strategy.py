import abc
from typing import Protocol, TypeVar
from concurrent.futures import Future, ThreadPoolExecutor
from arcadio_client import game


PlayerId = TypeVar("PlayerId", contravariant=True)
GameAction = TypeVar("GameAction", covariant=True)
StateDiff = TypeVar("StateDiff", covariant=True)


G = TypeVar("G", bound=game.GameState, contravariant=True)


class Strategy(Protocol[G, PlayerId, GameAction, StateDiff]):
    @abc.abstractmethod
    def take_action(self, game_state: G, player_id: PlayerId) -> GameAction | None: ...


class SlowStrategy(Strategy[G, PlayerId, GameAction, StateDiff]):
    """A strategy where each action takes longer to compute than the server's tick rate."""

    def __init__(self, strategy: Strategy[G, PlayerId, GameAction, StateDiff]) -> None:
        self.strategy = strategy
        self.action_job: Future[GameAction | None] | None = None

    def take_action(self, game_state: G, player_id: PlayerId) -> GameAction | None:
        if self.action_job is None:
            self.action_job = ThreadPoolExecutor().submit(self.strategy.take_action, game_state, player_id)
            return None
        elif self.action_job.done():
            action = self.action_job.result()
            self.action_job = None
            return action
        else:
            return None
