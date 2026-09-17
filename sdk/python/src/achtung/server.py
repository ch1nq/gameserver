"""gRPC serving loop: implements the `achtung.agent` game service for an `Agent`.

Pacing is fully automatic. The handler reads every tick the host sends and
answers every tick immediately with the latest finished action, while at most
one `Agent.step` call runs in the background on the newest state. Fast agents get
a fresh decision (nearly) every tick; slow agents act every N ticks without any
special-casing. A `step` that raises or returns garbage never breaks the
stream — the previous action is held, since the host eliminates agents whose
stream breaks but tolerates stale answers.
"""

from __future__ import annotations

import asyncio
import logging
import os
from collections.abc import AsyncIterator

import grpc

from achtung._generated import achtung_agent_pb2 as pb2
from achtung._generated import achtung_agent_pb2_grpc as pb2_grpc
from achtung.agent import Agent
from achtung.types import Action, ArenaConfig, arena_from_proto, game_state_from_proto

__all__ = ["DEFAULT_PORT", "resolve_port", "run"]

logger = logging.getLogger(__name__)

DEFAULT_PORT = 50052
DEFAULT_ARENA = ArenaConfig(width=1000, height=1000)

# Module-level constants are typed as Direction.ValueType (a NewType over int),
# which is what AgentAction(direction=...) requires; plain ints are rejected.
_ACTION_TO_DIRECTION = {
    Action.STRAIGHT: pb2.DIRECTION_STAIGHT,
    Action.LEFT: pb2.DIRECTION_TURN_LEFT,
    Action.RIGHT: pb2.DIRECTION_TURN_RIGHT,
}


class _AgentServicer(pb2_grpc.AgentServicer):
    """Serves a single game for one agent."""

    def __init__(self, agent: Agent) -> None:
        # Agent is structural (Protocol), so a missing step would otherwise
        # surface as an AttributeError mid-game. Fail fast with a clear error.
        if not callable(getattr(agent, "step", None)):
            raise TypeError(f"agent must define step(state) -> Action, got {type(agent).__name__}")
        self._agent = agent
        self._me_id = 0
        self._arena = DEFAULT_ARENA

    async def Initialize(
        self, request: pb2.InitializeRequest, context: grpc.aio.ServicerContext
    ) -> pb2.InitializeResponse:
        self._me_id = int(request.player_id)
        num_players = int(request.num_players)
        self._arena = (
            arena_from_proto(request.arena) if request.HasField("arena") else DEFAULT_ARENA
        )
        logger.info(
            "initialized as player %d of %d, arena %dx%d",
            self._me_id,
            num_players,
            self._arena.width,
            self._arena.height,
        )
        return pb2.InitializeResponse()

    async def Play(
        self,
        request_iterator: AsyncIterator[pb2.PlayRequest],
        context: grpc.aio.ServicerContext,
    ) -> AsyncIterator[pb2.PlayResponse]:
        # Answer the stream immediately: the host resolves its opening `Play`
        # call on response headers, which grpc only sends once this handler
        # starts yielding — but the host pushes tick 0 only after *every*
        # agent's stream is open. Without this, agenth sides wait forever.
        await context.send_initial_metadata(())
        loop = asyncio.get_running_loop()
        latest = Action.STRAIGHT
        pending: asyncio.Future[Action] | None = None

        def on_done(future: asyncio.Future[Action]) -> None:
            nonlocal latest, pending
            pending = None
            try:
                result = future.result()
            except Exception:
                logger.exception("Agent.step raised; holding %s", latest)
                return
            if isinstance(result, Action):
                latest = result
            else:
                logger.warning("Agent.step returned %r; holding %s", result, latest)

        async for request in request_iterator:
            state = game_state_from_proto(request, self._me_id, self._arena)
            if pending is None:
                future = loop.run_in_executor(None, self._agent.step, state)
                future.add_done_callback(on_done)
                pending = future
            yield pb2.PlayResponse(
                tick=request.tick,
                action=pb2.AgentAction(direction=_ACTION_TO_DIRECTION[latest]),
            )


async def _serve(agent: Agent, port: int) -> None:
    server = grpc.aio.server()
    pb2_grpc.add_AgentServicer_to_server(_AgentServicer(agent), server)
    bound = server.add_insecure_port(f"0.0.0.0:{port}")
    if bound == 0:
        raise OSError(f"could not bind port {port}")
    logger.info("achtung agent listening on 0.0.0.0:%d", bound)
    await server.start()
    await server.wait_for_termination()


def resolve_port(explicit: int | None = None) -> int:
    """Server port: `PORT` env wins (container convention), then `explicit`, then 50052."""
    raw = os.environ.get("PORT")
    if raw:
        return int(raw)
    return explicit if explicit is not None else DEFAULT_PORT


def run(agent: Agent, port: int | None = None) -> None:
    """Serve `agent` forever (blocking)."""
    logging.basicConfig(level=logging.INFO)
    asyncio.run(_serve(agent, resolve_port(port)))
