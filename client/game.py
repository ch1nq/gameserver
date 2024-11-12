import abc
from typing import Protocol, TypeVar

import attrs


@attrs.define
class GameResult:
    winner: str
    score: int


PlayerIdT = TypeVar("PlayerIdT")
GameActionT = TypeVar("GameActionT")
StateDiffT = TypeVar("StateDiffT")


class GameState(Protocol[PlayerIdT, GameActionT, StateDiffT]):
    player_id_type: type[PlayerIdT]
    game_action_type: type[GameActionT]
    state_diff_type: type[StateDiffT]

    @abc.abstractmethod
    def merge_with_diff(self, diff: StateDiffT) -> None: ...

    def game_over_callback(self, winner: PlayerIdT) -> None:
        pass
