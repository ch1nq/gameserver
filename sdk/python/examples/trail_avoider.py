"""Example agent: avoid the trails everyone leaves behind.

Shows off the trail data the SDK now delivers ready-made: `state` carries every
player's full trail (`PlayerState.trail`), so the agent never has to accumulate
head positions itself. Here we look a short distance ahead of our head and, if
the straight path would run into any trail point (ours or an opponent's) or a
wall, we turn.

Run locally against a game host pointed at this agent:

    uv run python examples/trail_avoider.py
"""

import math

from achtung import Action, Agent, GameState, Position, run

# How far ahead of the head to probe, in game units.
LOOKAHEAD = 24.0


def _hits(point: Position, targets, radius: float) -> bool:
    return any((point.x - t.x) ** 2 + (point.y - t.y) ** 2 < radius * radius for t in targets)


class TrailAvoider(Agent):
    def step(self, state: GameState) -> Action:
        me = state.me()
        ahead = Position(
            x=me.position.x + math.cos(me.direction) * LOOKAHEAD,
            y=me.position.y + math.sin(me.direction) * LOOKAHEAD,
        )

        # Off the arena ahead? Turn.
        if not (0 <= ahead.x <= state.arena.width and 0 <= ahead.y <= state.arena.height):
            return Action.LEFT

        # Everyone's trail is a wall; skip the tail of our own trail, which the
        # engine ignores for self-collision so we don't dodge the point we just
        # left. Opponent heads are hazards too.
        radius = me.size * 2
        for player in state.players:
            is_me = player.player_id == state.me_id
            trail = player.trail[:-10] if is_me else player.trail
            if _hits(ahead, trail, radius):
                return Action.LEFT
            if not is_me and _hits(ahead, (player.position,), radius):
                return Action.LEFT

        return Action.STRAIGHT


if __name__ == "__main__":
    run(TrailAvoider())
