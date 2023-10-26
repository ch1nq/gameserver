import asyncio
import random
import time

import game
import strategy
import game_client


class RandomStrategy(strategy.Strategy):
    def take_action(self, _game_state: game.GameState, player_id: game.PlayerId) -> game.GameAction | None:
        action = random.choice(list(game.GameAction))
        time.sleep(abs(random.gauss(0.2, 0.05)))
        # if player_id == 1:
        #     print("Taking action ", action)
        return action


async def main():
    strat = strategy.SlowStrategy(RandomStrategy())
    client = await game_client.GameClient("127.0.0.1", 3030, strat).connect()
    await client.run()


async def run_clients() -> None:
    # run multiple clients concurrently
    tasks = []
    for _ in range(4):
        tasks.append(asyncio.create_task(main()))

    await asyncio.gather(*tasks)


if __name__ == "__main__":
    asyncio.run(run_clients())
