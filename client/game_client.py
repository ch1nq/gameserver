from typing import Literal

import attrs
import game
import pydantic
import strategy
import websockets


class GameOver(pydantic.BaseModel):
    event_type: Literal["GameOver"]
    winner: game.PlayerId


class UpdateState(pydantic.BaseModel):
    event_type: Literal["UpdateState"]
    new_state: game.GameState


class AssignPlayerId(pydantic.BaseModel):
    event_type: Literal["AssignPlayerId"]
    player_id: game.PlayerId


GameEventT = GameOver | UpdateState | AssignPlayerId


class GameEvent(pydantic.BaseModel):
    event: GameEventT = pydantic.Field(..., discriminator="event_type")


class ActionEvent(pydantic.BaseModel):
    event_type: Literal["Action"] = "Action"
    action: game.GameAction


class RequestUpdateEvent(pydantic.BaseModel):
    event_type: Literal["RequestUpdate"] = "RequestUpdate"


PlayerEventT = ActionEvent | RequestUpdateEvent


@attrs.define(kw_only=True)
class GameClient:
    game_strategy: strategy.Strategy = attrs.field()
    request_updates: bool = attrs.field(default=False)

    async def connect(self, host: str, port: int) -> "ConnectedGameClient":
        connection = await websockets.connect(f"ws://{host}:{port}/join/player")
        return ConnectedGameClient(connection=connection, **attrs.asdict(self))


@attrs.define(kw_only=True)
class ConnectedGameClient(GameClient):
    _connection: websockets.WebSocketClientProtocol = attrs.field()
    _game_state: game.GameState = attrs.field(factory=game.GameState.default, init=False)

    async def send_event(self, player_event: PlayerEventT) -> None:
        await self._connection.send(player_event.model_dump_json())

    async def receive_event(self) -> GameEventT:
        match await self._connection.recv():
            case bytes() as data:
                event_data = data.decode("utf-8")
            case str() as data:
                event_data = data
            case data:
                raise ValueError(f"Unexpected type {type(data)}")
        return GameEvent.model_validate_json(event_data).event

    async def run(self) -> None:
        # Expect server to assign us a player id before the game starts
        match await self.receive_event():
            case AssignPlayerId(player_id=id):
                player_id = id
            case event:
                raise ValueError(f"Expected 'AssignPlayerId' event, but got '{event}'")

        while True:
            if self.request_updates:
                await self.send_event(RequestUpdateEvent())
            match await self.receive_event():
                case UpdateState(new_state=state):
                    self._game_state = self._game_state.merge_with_diff(state)
                    action = self.game_strategy.take_action(state, player_id)
                    if action is not None:
                        await self.send_event(ActionEvent(action=action))
                case GameOver(winner=player_id):
                    print(f"Game over! {player_id} won!")
                    await self._connection.close()
                    break
