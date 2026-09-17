"""Smoke test: fake game host driving the SDK server end to end."""

import asyncio
from collections.abc import AsyncIterator
from typing import cast

import grpc
import pytest

from achtung._generated import achtung_agent_pb2 as pb2
from achtung._generated import achtung_agent_pb2_grpc as pb2_grpc
from achtung.agent import Agent
from achtung.server import _AgentServicer
from achtung.types import Action, GameState

LEFT_PROTO = 2


class LeftAgent(Agent):
    def __init__(self) -> None:
        self.seen_ticks: list[int] = []

    def step(self, state: GameState) -> Action:
        self.seen_ticks.append(state.tick)
        return Action.LEFT


def _player(player_id: int) -> pb2.PlayerState:
    return pb2.PlayerState(
        player_id=player_id,
        position=pb2.Position(x=500.0, y=500.0),
        direction=0.0,
        alive=True,
    )


async def _serve(agent: Agent) -> tuple[grpc.aio.Server, int]:
    server = grpc.aio.server()
    pb2_grpc.add_AgentServicer_to_server(_AgentServicer(agent), server)
    port = server.add_insecure_port("127.0.0.1:0")
    await server.start()
    return server, port


async def test_initialize_and_play_tick_echo() -> None:
    agent = LeftAgent()
    server, port = await _serve(agent)
    try:
        async with grpc.aio.insecure_channel(f"127.0.0.1:{port}") as channel:
            stub = pb2_grpc.AgentStub(channel)
            await stub.Initialize(
                pb2.InitializeRequest(
                    player_id=1,
                    num_players=2,
                    arena=pb2.ArenaConfig(width=1000, height=800),
                )
            )

            async def requests() -> AsyncIterator[pb2.PlayRequest]:
                for tick in range(1, 4):
                    yield pb2.PlayRequest(
                        tick=tick,
                        state=pb2.GameState(tick=tick, players=[_player(0), _player(1)]),
                    )

            responses = [response async for response in stub.Play(requests())]
    finally:
        await server.stop(None)

    # Every request is answered with its own tick echoed back.
    assert [r.tick for r in responses] == [1, 2, 3]
    # The agent's decision lands on the stream (pipelined: the first answer may
    # still be the STRAIGHT default while the first step call runs, and fast
    # ticks may coalesce onto one step call).
    assert responses[-1].action.direction == LEFT_PROTO
    assert agent.seen_ticks and agent.seen_ticks[0] == 1


async def test_play_stream_opens_before_any_request() -> None:
    # The host resolves its opening Play call on response headers, before it
    # pushes tick 0 (which happens only after every agent's stream is open).
    # If headers waited for the first request, agenth sides would deadlock, and
    # the host would fail setup after 5s.
    agent = LeftAgent()
    server, port = await _serve(agent)
    try:
        async with grpc.aio.insecure_channel(f"127.0.0.1:{port}") as channel:
            stub = pb2_grpc.AgentStub(channel)
            await stub.Initialize(pb2.InitializeRequest(player_id=0, num_players=1))
            gate = asyncio.Event()

            async def no_requests_yet() -> AsyncIterator[pb2.PlayRequest]:
                await gate.wait()
                yield pb2.PlayRequest(tick=0)

            call = stub.Play(no_requests_yet())
            await asyncio.wait_for(call.initial_metadata(), timeout=5)
            call.cancel()
            gate.set()
    finally:
        await server.stop(None)


async def test_raising_agent_holds_default_and_survives() -> None:
    class RaisingAgent(Agent):
        def step(self, state: GameState) -> Action:
            raise RuntimeError("boom")

    server, port = await _serve(RaisingAgent())
    try:
        async with grpc.aio.insecure_channel(f"127.0.0.1:{port}") as channel:
            stub = pb2_grpc.AgentStub(channel)
            await stub.Initialize(pb2.InitializeRequest(player_id=0, num_players=1))

            async def requests() -> AsyncIterator[pb2.PlayRequest]:
                for tick in range(1, 3):
                    yield pb2.PlayRequest(tick=tick, state=pb2.GameState(tick=tick))

            responses = [response async for response in stub.Play(requests())]
    finally:
        await server.stop(None)

    assert [r.tick for r in responses] == [1, 2]
    assert all(r.action.direction == 1 for r in responses)  # STRAIGHT default held


async def test_duck_typed_agent_needs_no_base_class() -> None:
    # Agent is a Protocol: any object with step(state) -> Action works.

    class PlainAgent:
        def step(self, state: GameState) -> Action:
            return Action.RIGHT

    server, port = await _serve(PlainAgent())
    try:
        async with grpc.aio.insecure_channel(f"127.0.0.1:{port}") as channel:
            stub = pb2_grpc.AgentStub(channel)
            await stub.Initialize(pb2.InitializeRequest(player_id=0, num_players=1))

            async def requests() -> AsyncIterator[pb2.PlayRequest]:
                for tick in range(1, 3):
                    yield pb2.PlayRequest(tick=tick, state=pb2.GameState(tick=tick))

            responses = [response async for response in stub.Play(requests())]
    finally:
        await server.stop(None)

    assert [r.tick for r in responses] == [1, 2]
    assert responses[-1].action.direction == 3  # RIGHT


async def test_agent_without_step_fails_fast() -> None:
    with pytest.raises(TypeError, match="must define step"):
        _AgentServicer(cast(Agent, object()))
