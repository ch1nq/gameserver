"""Example agent: drive straight, turn left near walls.

Raw-state only: it steers on its own head position versus the arena bounds.
Run locally against a game host pointed at this agent:

    uv run python examples/wall_avoider.py
"""

from achtung import Action, Agent, GameState, run

MARGIN = 100.0


class WallAvoider(Agent):
    def step(self, state: GameState) -> Action:
        me = state.me()
        near_left = me.position.x < MARGIN
        near_right = me.position.x > state.arena.width - MARGIN
        near_top = me.position.y < MARGIN
        near_bottom = me.position.y > state.arena.height - MARGIN
        if near_left or near_right or near_top or near_bottom:
            return Action.LEFT
        return Action.STRAIGHT


if __name__ == "__main__":
    run(WallAvoider())
