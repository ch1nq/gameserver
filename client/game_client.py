from typing import Literal
import websockets
import pydantic

import game
import strategy


class GameOver(pydantic.BaseModel):
    event_type: Literal["GameOver"]
    winner: game.PlayerId


class UpdateState(pydantic.BaseModel):
    event_type: Literal["UpdateState"]
    new_state: game.GameState


class GameEvent(pydantic.BaseModel):
    event: GameOver | UpdateState = pydantic.Field(..., discriminator="event_type")


class ActionEvent(pydantic.BaseModel):
    event_type: Literal["Action"] = "Action"
    action: game.GameAction


class JoinEvent(pydantic.BaseModel):
    event_type: Literal["Join"] = "Join"


class LeaveEvent(pydantic.BaseModel):
    event_type: Literal["Leave"] = "Leave"


PlayerEvent = ActionEvent | JoinEvent | LeaveEvent


class GameClient:
    def __init__(self, host: str, port: int, strategy: strategy.Strategy):
        self.host = host
        self.port = port
        self.strategy = strategy

    async def connect(self) -> "ConnectedGameClient":
        uri = f"ws://{self.host}:{self.port}/game"
        connection = await websockets.connect(uri)
        return ConnectedGameClient(connection, self.strategy)


class ConnectedGameClient(GameClient):
    def __init__(self, connection: websockets.WebSocketClientProtocol, strategy: strategy.Strategy):
        self.connection = connection
        self.strategy = strategy
        self.game_state: game.GameState | None = None

    async def send_event(self, player_event: PlayerEvent) -> None:
        await self.connection.send(player_event.model_dump_json())

    async def receive_event(self) -> GameEvent:
        match await self.connection.recv():
            case bytes() as data:
                event_data = data.decode("utf-8")
            case str() as data:
                event_data = data
            case data:
                raise ValueError(f"Unexpected type {type(data)}")
        return GameEvent.model_validate_json(event_data)

    async def run(self) -> None:
        while True:
            event = await self.receive_event()
            match event.event:
                case UpdateState(new_state=state):
                    self.game_state = self.game_state.merge_with_diff(state) if self.game_state else state
                    if action := self.strategy.take_action(state):
                        await self.send_event(ActionEvent(action=action))
                case GameOver(winner=player_id):
                    print(f"Game over! {player_id} won!")
                    await self.connection.close()
                    break
