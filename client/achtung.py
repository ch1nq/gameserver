import attrs
import enum
from typing import NewType

PlayerId = NewType("PlayerId", int)
BlobId = NewType("BlobId", int)


@attrs.define
class Angle:
    radians: float


@attrs.define
class Position:
    x: float
    y: float


@attrs.define
class Blob:
    id: BlobId
    size: float
    position: Position


@enum.unique
class GameAction(enum.Enum):
    LEFT = "Left"
    RIGHT = "Right"
    FORWARD = "Forward"


@attrs.define
class Player:
    is_alive: bool
    head: Blob
    body: list[Blob]
    direction: Angle
    speed: float
    turning_speed: float
    size: float
    action: GameAction
    skip_frequency: int
    skip_duration: int


@attrs.define
class GameState:
    timestep: int
    players: dict[PlayerId, Player]

    def merge_with_diff(self, diff: "GameStateDiff") -> None:
        self.timestep = diff.timestep
        for id, player_diff in diff.players.items():
            match (self.players.get(id), player_diff.body):
                case (None, _) | (_, None):
                    continue
                case (player, body_diff):
                    player.body.extend(body_diff)
            # TODO: handle other fields


@attrs.define
class PlayerDiff:
    is_alive: bool | None = None
    head: Blob | None = None
    body: list[Blob] | None = None
    direction: Angle | None = None
    speed: float | None = None
    turning_speed: float | None = None
    size: float | None = None
    action: GameAction | None = None
    skip_frequency: int | None = None
    skip_duration: int | None = None


@attrs.define
class GameStateDiff:
    timestep: int
    players: dict[PlayerId, PlayerDiff]
