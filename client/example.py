import asyncio
import random
import achtung
import strategy
import game_client


class RandomStrategy(strategy.Strategy):
    def take_action(self, _game_state: achtung.GameState, player_id: achtung.PlayerId) -> achtung.GameAction | None:
        action = random.choice(list(achtung.GameAction))
        return action


async def create_client(request_updates: bool):
    strat = RandomStrategy()
    client = await game_client.GameClient(game_strategy=strat, request_updates=request_updates).connect(
        "127.0.0.1", 3030
    )
    await client.run()


async def run_clients() -> None:
    # run multiple clients concurrently
    tasks = []
    for i in range(8):
        tasks.append(asyncio.create_task(create_client(request_updates=False)))  # i == 0)))

    await asyncio.gather(*tasks)


if __name__ == "__main__":
    asyncio.run(run_clients())
