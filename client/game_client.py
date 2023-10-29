import json
from typing import Literal, TypeVar, Generic

import attrs
import strategy
import websockets
import cattrs
import game


PlayerIdT = TypeVar("PlayerIdT")
GameActionT = TypeVar("GameActionT")
StateDiffT = TypeVar("StateDiffT")
G = TypeVar("G", bound=game.GameState)


@attrs.define
class GameOver(Generic[PlayerIdT]):
    e: Literal["GameOver"]
    winner: PlayerIdT


@attrs.define
class UpdateState(Generic[StateDiffT]):
    e: Literal["UpdateState"]
    diff: StateDiffT


@attrs.define
class InitialState(Generic[G]):
    e: Literal["InitialState"]
    state: G


@attrs.define
class AssignPlayerId(Generic[PlayerIdT]):
    e: Literal["AssignPlayerId"]
    player_id: PlayerIdT


GameEventT = InitialState[G] | AssignPlayerId[PlayerIdT] | UpdateState[StateDiffT] | GameOver[PlayerIdT]


@attrs.define
class GameEvent(Generic[G, PlayerIdT, StateDiffT]):
    event: GameEventT[G, PlayerIdT, StateDiffT]


@attrs.define
class ActionEvent(Generic[GameActionT]):
    action: GameActionT
    e: Literal["Action"] = attrs.field(default="Action")


@attrs.define
class RequestUpdateEvent:
    e: Literal["RequestUpdate"] = attrs.field(default="RequestUpdate")


PlayerEventT = ActionEvent[GameActionT] | RequestUpdateEvent


@attrs.define(kw_only=True)
class GameClient(Generic[G, PlayerIdT, GameActionT, StateDiffT]):
    game_strategy: strategy.Strategy = attrs.field()
    request_updates: bool = attrs.field(default=False)
    game_state_type: type[G] = attrs.field()

    async def connect(self, host: str, port: int) -> "ConnectedGameClient[G, PlayerIdT, GameActionT, StateDiffT]":
        connection = await websockets.connect(f"ws://{host}:{port}/join/player")
        return ConnectedGameClient(connection=connection, **attrs.asdict(self))  # type: ignore

    def serialize_player_event(self, event: PlayerEventT[GameActionT]) -> bytes:
        return json.dumps(cattrs.unstructure(event)).encode("utf-8")

    def deserialize_game_event(self, data: bytes) -> "GameEvent[G, PlayerIdT, StateDiffT]":
        return cattrs.structure(
            json.loads(data),
            GameEvent[self.game_state_type, self.game_state_type.player_id_type, self.game_state_type.state_diff_type],
        )


@attrs.define(kw_only=True)
class ConnectedGameClient(GameClient[G, PlayerIdT, GameActionT, StateDiffT]):
    _connection: websockets.WebSocketClientProtocol = attrs.field()

    async def send_event(self, player_event: PlayerEventT) -> None:
        if self._connection.open:
            await self._connection.send(self.serialize_player_event(player_event))

    async def receive_event(self) -> GameEventT[G, PlayerIdT, StateDiffT]:
        match await self._connection.recv():
            case str(data):
                return self.deserialize_game_event(data.encode("utf-8")).event
            case bytes(data):
                return self.deserialize_game_event(data).event
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
                    game_state.game_over_callback(winner=player_id)
                    await self._connection.close()
                    break
