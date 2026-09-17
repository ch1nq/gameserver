"""Unit tests for Action mapping and proto conversion (no generated code needed)."""

from types import SimpleNamespace

from achtung.types import (
    Action,
    action_from_proto,
    action_to_proto,
    arena_from_proto,
    coerce_action,
    game_state_from_proto,
)


def test_action_to_proto_matches_direction_enum() -> None:
    assert action_to_proto(Action.STRAIGHT) == 1
    assert action_to_proto(Action.LEFT) == 2
    assert action_to_proto(Action.RIGHT) == 3


def test_action_to_proto_coerces_garbage_to_straight() -> None:
    assert action_to_proto("left") == 1
    assert action_to_proto(None) == 1
    assert action_to_proto(2) == 1


def test_action_from_proto() -> None:
    assert action_from_proto(0) == Action.STRAIGHT  # unspecified
    assert action_from_proto(1) == Action.STRAIGHT
    assert action_from_proto(2) == Action.LEFT
    assert action_from_proto(3) == Action.RIGHT
    assert action_from_proto(99) == Action.STRAIGHT


def test_coerce_action() -> None:
    assert coerce_action(Action.LEFT) is Action.LEFT
    assert coerce_action(-1) is Action.STRAIGHT
    assert coerce_action(None) is Action.STRAIGHT


def test_game_state_from_proto() -> None:
    request = SimpleNamespace(
        tick=7,
        state=SimpleNamespace(
            players=[
                SimpleNamespace(
                    player_id=0,
                    position=SimpleNamespace(x=10.0, y=20.0),
                    direction=0.5,
                    alive=True,
                ),
                SimpleNamespace(
                    player_id=1,
                    position=None,
                    direction=0.0,
                    alive=False,
                ),
            ]
        ),
    )
    arena = arena_from_proto(SimpleNamespace(width=1000, height=800))
    state = game_state_from_proto(request, me_id=0, arena=arena)
    assert state.tick == 7
    assert state.me_id == 0
    assert state.arena.width == 1000
    me = state.me()
    assert (me.position.x, me.position.y) == (10.0, 20.0)
    assert me.direction == 0.5
    assert me.alive
    assert [p.player_id for p in state.others()] == [1]
    assert state.get(999) is None


def test_game_state_from_proto_missing_state() -> None:
    state = game_state_from_proto(
        SimpleNamespace(tick=3, state=None), me_id=0, arena=arena_from_proto(None)
    )
    assert state.tick == 3
    assert state.players == ()
    assert (state.arena.width, state.arena.height) == (0, 0)
