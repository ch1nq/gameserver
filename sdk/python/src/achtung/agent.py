"""Agent interface: the one function an agent author implements."""

from __future__ import annotations

from typing import Protocol

from achtung.types import Action, GameState

__all__ = ["Agent"]


class Agent(Protocol):
    """Agent logic.

    A structural interface: subclassing is optional, any object with a
    `step(state) -> Action` method works. `step` receives the newest game
    state and returns a steering `Action`. It runs off the event loop, so
    blocking compute is safe — but while a `step` call is still running, the
    server keeps answering every tick with the latest finished action. Slow
    steps therefore mean acting every N ticks automatically, with no extra
    code.
    """

    def step(self, state: GameState) -> Action:
        """Compute the steering action for the newest game state."""
        ...
