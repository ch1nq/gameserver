import pydantic
import enum
from typing import NewType

PlayerId = NewType("PlayerId", int)
BlobId = NewType("BlobId", int)


class Angle(pydantic.BaseModel):
    radians: float


class Position(pydantic.BaseModel):
    x: float
    y: float


class Blob(pydantic.BaseModel):
    id: BlobId
    size: float
    position: Position


class GameAction(enum.Enum):
    LEFT = "Left"
    RIGHT = "Right"
    FORWARD = "Forward"


class Player(pydantic.BaseModel):
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


class GameState(pydantic.BaseModel):
    timestep: int
    players: dict[PlayerId, Player]

    @classmethod
    def default(cls) -> "GameState":
        return GameState(timestep=0, players={})

    def merge_with_diff(self, diff: "GameState") -> "GameState":
        for id, player in diff.players.items():
            if id in self.players:
                player.body.extend(self.players[id].body)
        return diff
