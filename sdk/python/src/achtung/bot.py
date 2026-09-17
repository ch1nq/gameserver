"""Bot interface: the one function an agent author implements."""

from __future__ import annotations

import abc

from achtung.types import Action, GameState

__all__ = ["Bot"]


class Bot(abc.ABC):
    """Agent logic.

    `step` receives the newest game state and returns a steering `Action`.
    It runs off the event loop, so blocking compute is safe — but while a
    `step` call is still running, the server keeps answering every tick with
    the latest finished action. Slow steps therefore mean acting every N
    ticks automatically, with no extra code.
    """

    @abc.abstractmethod
    def step(self, state: GameState) -> Action:
        """Compute the steering action for the newest game state."""
        raise NotImplementedError
