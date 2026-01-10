import attrs
import enum
from typing import NewType
from arcadio_client import game

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
class AchtungDiff:
    timestep: int
    players: dict[PlayerId, PlayerDiff]


@attrs.define
class Achtung(game.GameState):
    timestep: int
    players: dict[PlayerId, Player]
    player_id_type = PlayerId
    game_action_type = GameAction
    state_diff_type = AchtungDiff

    def merge_with_diff(self, diff: AchtungDiff) -> None:
        self.timestep = diff.timestep
        for id, player_diff in diff.players.items():
            match (self.players.get(id), player_diff.body):
                case (None, _) | (_, None):
                    continue
                case (player, body_diff):
                    player.body.extend(body_diff)
            # TODO: handle other fields

    def game_over_callback(self, winner: PlayerId) -> None:
        print(f"Game over! {winner} won after {self.timestep} timesteps.")
