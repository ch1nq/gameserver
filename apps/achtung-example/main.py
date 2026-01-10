import asyncio
import logging
import os
import random

from arcadio_client import achtung
from arcadio_client import client
from arcadio_client import strategy


def main():
    logging.basicConfig(level=logging.INFO)
    asyncio.run(run_client_indefinitely())


class RandomStrategy(strategy.Strategy):
    def take_action(self, _game_state: achtung.Achtung, player_id: achtung.PlayerId) -> achtung.GameAction | None:
        action = random.choice(list(achtung.GameAction))
        return action


async def create_client(request_updates: bool) -> client.ConnectedGameClient:
    strat = RandomStrategy()
    host = os.environ.get("SERVER_HOST", default="localhost")
    port = int(os.environ.get("SERVER_PORT", default=3030))
    return await client.GameClient(
        game_state_type=achtung.Achtung,
        game_strategy=strat,
        request_updates=request_updates,
    ).connect(host, port)


async def run_client_indefinitely() -> None:
    while True:
        client = await create_client(request_updates=False)
        await client.run()


if __name__ == "__main__":
    main()
