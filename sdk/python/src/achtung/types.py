"""Core data types for the Achtung agent SDK.

`Action` is what `Agent.step` returns. The `GameState`/`PlayerState` dataclasses
are plain snapshots of the newest tick the host sent; conversion helpers map
them to and from the generated proto messages without this module importing
the generated code.
"""

from __future__ import annotations

import enum
from dataclasses import dataclass
from typing import Any

__all__ = [
    "Action",
    "ArenaConfig",
    "GameState",
    "PlayerState",
    "Position",
    "action_from_proto",
    "action_to_proto",
    "arena_from_proto",
    "coerce_action",
    "game_state_from_proto",
]


class Action(enum.Enum):
    """Steering decision returned by `Agent.step`."""

    STRAIGHT = "straight"
    LEFT = "left"
    RIGHT = "right"


# Proto `achtung.agent.Direction` values (protos/achtung_agent.proto).
_STRAIGHT_PROTO = 1
_LEFT_PROTO = 2
_RIGHT_PROTO = 3

_ACTION_TO_PROTO = {
    Action.STRAIGHT: _STRAIGHT_PROTO,
    Action.LEFT: _LEFT_PROTO,
    Action.RIGHT: _RIGHT_PROTO,
}

_PROTO_TO_ACTION = {
    0: Action.STRAIGHT,  # DIRECTION_UNSPECIFIED
    _STRAIGHT_PROTO: Action.STRAIGHT,
    _LEFT_PROTO: Action.LEFT,
    _RIGHT_PROTO: Action.RIGHT,
}


def action_to_proto(action: object) -> int:
    """Map an `Action` to its proto `Direction` value.

    Anything that is not an `Action` coerces to straight, matching the host's
    fallback for unknown directions.
    """
    if isinstance(action, Action):
        return _ACTION_TO_PROTO[action]
    return _STRAIGHT_PROTO


def action_from_proto(direction: int) -> Action:
    """Map a proto `Direction` value to an `Action`; unknown values go straight."""
    return _PROTO_TO_ACTION.get(direction, Action.STRAIGHT)


def coerce_action(value: object) -> Action:
    """Coerce a `Agent.step` return value to `Action`; anything else goes straight."""
    return value if isinstance(value, Action) else Action.STRAIGHT


@dataclass(frozen=True)
class ArenaConfig:
    width: int
    height: int


@dataclass(frozen=True)
class Position:
    x: float
    y: float


@dataclass(frozen=True)
class PlayerState:
    player_id: int
    position: Position
    # Heading in radians.
    direction: float
    alive: bool


@dataclass(frozen=True)
class GameState:
    """Snapshot of the newest tick received from the host."""

    tick: int
    players: tuple[PlayerState, ...]
    me_id: int
    arena: ArenaConfig

    def me(self) -> PlayerState:
        """This agent's own player state; raises `LookupError` if absent."""
        player = self.get(self.me_id)
        if player is None:
            raise LookupError(f"own player id {self.me_id} missing from game state")
        return player

    def get(self, player_id: int) -> PlayerState | None:
        for player in self.players:
            if player.player_id == player_id:
                return player
        return None

    def others(self) -> tuple[PlayerState, ...]:
        return tuple(p for p in self.players if p.player_id != self.me_id)


def arena_from_proto(arena: Any) -> ArenaConfig:
    return ArenaConfig(
        width=int(getattr(arena, "width", 0) or 0),
        height=int(getattr(arena, "height", 0) or 0),
    )


def game_state_from_proto(request: Any, me_id: int, arena: ArenaConfig) -> GameState:
    state = getattr(request, "state", None)
    raw_players = getattr(state, "players", ()) if state is not None else ()
    players = tuple(_player_from_proto(p) for p in raw_players)
    return GameState(
        tick=int(getattr(request, "tick", 0) or 0),
        players=players,
        me_id=me_id,
        arena=arena,
    )


def _player_from_proto(player: Any) -> PlayerState:
    position = getattr(player, "position", None)
    return PlayerState(
        player_id=int(player.player_id),
        position=Position(
            x=float(position.x) if position is not None else 0.0,
            y=float(position.y) if position is not None else 0.0,
        ),
        direction=float(getattr(player, "direction", 0.0)),
        alive=bool(getattr(player, "alive", False)),
    )
