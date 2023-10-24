import asyncio
import random
import time

import game
import strategy
import game_client


class RandomStrategy(strategy.Strategy):
    def take_action(self, _game_state: game.GameState) -> game.GameAction | None:
        time.sleep(abs(random.gauss(0.2, 0.05)))
        return random.choice(list(game.GameAction))


async def main():
    strat = strategy.SlowStrategy(RandomStrategy())
    client = await game_client.GameClient("127.0.0.1", 3030, strat).connect()
    await client.run()


async def run_clients() -> None:
    # run three clients concurrently
    await asyncio.gather(*[main() for _ in range(7)])


if __name__ == "__main__":
    asyncio.run(run_clients())
