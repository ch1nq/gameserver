"""Slow-agent test: a step slower than the tick rate acts every N ticks.

The stream must stay healthy throughout: every request gets its tick echoed,
thinking coalesces (one step call covers many ticks), and the finished
decision lands on later responses.
"""

import asyncio
import time
from collections.abc import AsyncIterator

import grpc

from achtung._generated import achtung_agent_pb2 as pb2
from achtung._generated import achtung_agent_pb2_grpc as pb2_grpc
from achtung.agent import Agent
from achtung.server import _AgentServicer
from achtung.types import Action, GameState

STEP_DELAY = 0.2
TICK_SPACING = 0.02
STRAIGHT_PROTO = 1
RIGHT_PROTO = 3


class SlowAgent(Agent):
    def __init__(self) -> None:
        self.calls = 0

    def step(self, state: GameState) -> Action:
        self.calls += 1
        time.sleep(STEP_DELAY)
        return Action.RIGHT


async def test_slow_step_coalesces_ticks_and_lands_later() -> None:
    agent = SlowAgent()
    server = grpc.aio.server()
    pb2_grpc.add_AgentServicer_to_server(_AgentServicer(agent), server)
    port = server.add_insecure_port("127.0.0.1:0")
    await server.start()
    try:
        async with grpc.aio.insecure_channel(f"127.0.0.1:{port}") as channel:
            stub = pb2_grpc.AgentStub(channel)
            await stub.Initialize(pb2.InitializeRequest(player_id=0, num_players=1))

            async def ticks() -> AsyncIterator[pb2.PlayRequest]:
                # Six ticks arrive in ~0.12s while one step takes 0.2s.
                for tick in range(1, 7):
                    yield pb2.PlayRequest(tick=tick, state=pb2.GameState(tick=tick))
                    await asyncio.sleep(TICK_SPACING)
                # Let the first think land before sending more ticks.
                await asyncio.sleep(STEP_DELAY + 0.1)
                for tick in range(7, 10):
                    yield pb2.PlayRequest(tick=tick, state=pb2.GameState(tick=tick))
                    await asyncio.sleep(TICK_SPACING)

            responses = [r async for r in stub.Play(ticks())]
    finally:
        await server.stop(None)

    assert [r.tick for r in responses] == [1, 2, 3, 4, 5, 6, 7, 8, 9]
    # First answer is the STRAIGHT default: no step has finished yet.
    assert responses[0].action.direction == STRAIGHT_PROTO
    # The finished decision lands on later ticks; each burst needed one think.
    assert responses[-1].action.direction == RIGHT_PROTO
    assert agent.calls == 2
