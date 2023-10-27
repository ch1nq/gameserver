import json
from typing import Literal

import attrs
import achtung
import strategy
import websockets
import cattrs


@attrs.define
class GameOver:
    e: Literal["GameOver"]
    winner: achtung.PlayerId


@attrs.define
class UpdateState:
    e: Literal["UpdateState"]
    diff: achtung.GameStateDiff


@attrs.define
class InitialState:
    e: Literal["InitialState"]
    state: achtung.GameState


@attrs.define
class AssignPlayerId:
    e: Literal["AssignPlayerId"]
    player_id: achtung.PlayerId


GameEventT = GameOver | UpdateState | InitialState | AssignPlayerId


@attrs.define
class GameEvent:
    event: GameEventT


@attrs.define
class ActionEvent:
    action: achtung.GameAction
    e: Literal["Action"] = attrs.field(default="Action")


@attrs.define
class RequestUpdateEvent:
    e: Literal["RequestUpdate"] = attrs.field(default="RequestUpdate")


PlayerEventT = ActionEvent | RequestUpdateEvent


def deserialize_game_event(data: bytes) -> GameEvent:
    return cattrs.structure(json.loads(data), GameEvent)


def serialize_player_event(event: PlayerEventT) -> bytes:
    return json.dumps(cattrs.unstructure(event)).encode("utf-8")


@attrs.define(kw_only=True)
class GameClient:
    game_strategy: strategy.Strategy = attrs.field()
    request_updates: bool = attrs.field(default=False)

    async def connect(self, host: str, port: int) -> "ConnectedGameClient":
        connection = await websockets.connect(f"ws://{host}:{port}/join/player")
        return ConnectedGameClient(connection=connection, **attrs.asdict(self))  # type: ignore


@attrs.define(kw_only=True)
class ConnectedGameClient(GameClient):
    _connection: websockets.WebSocketClientProtocol = attrs.field()

    async def send_event(self, player_event: PlayerEventT) -> None:
        if self._connection.open:
            await self._connection.send(serialize_player_event(player_event))

    async def receive_event(self) -> GameEventT:
        match await self._connection.recv():
            case str(data):
                return deserialize_game_event(data.encode("utf-8")).event
            case bytes(data):
                return deserialize_game_event(data).event
            case data:
                raise ValueError(f"Unexpected type {type(data)}")

    async def run(self) -> None:
        # Expect server to assign us a player id before the game starts
        match (await self.receive_event(), await self.receive_event()):
            case (AssignPlayerId(player_id=id), InitialState(state=initial_state)):
                player_id = id
                game_state = initial_state
            case (event1, event2):
                raise ValueError(f"Expected 'AssignPlayerId' followed by 'InitialState', but got '{event1}, {event2}'")

        while True:
            if self.request_updates:
                await self.send_event(RequestUpdateEvent())
            match await self.receive_event():
                case UpdateState(diff=state_diff):
                    game_state.merge_with_diff(state_diff)
                    action = self.game_strategy.take_action(game_state, player_id)
                    if action is not None:
                        await self.send_event(ActionEvent(action=action))
                case GameOver(winner=player_id):
                    print(f"Game over! {player_id} won after {game_state.timestep} timesteps.")
                    await self._connection.close()
                    break
