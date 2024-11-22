import asyncio
import logging
import os
import random

import achtung
import game_client
import strategy


class RandomStrategy(strategy.Strategy):
    def take_action(self, _game_state: achtung.Achtung, player_id: achtung.PlayerId) -> achtung.GameAction | None:
        action = random.choice(list(achtung.GameAction))
        return action


async def create_client(request_updates: bool) -> game_client.ConnectedGameClient:
    strat = RandomStrategy()
    host = os.environ.get("SERVER_HOST", default="localhost")
    port = int(os.environ.get("SERVER_PORT", default=3030))
    client = await game_client.GameClient(
        game_state_type=achtung.Achtung,
        game_strategy=strat,
        request_updates=request_updates,
    ).connect(host, port)
    return client


async def run_client_indefinitely() -> None:
    while True:
        client = await create_client(request_updates=False)
        await client.run()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    asyncio.run(run_client_indefinitely())
