import asyncio
import random
import achtung
import strategy
import game_client


class RandomStrategy(strategy.Strategy):
    def take_action(self, _game_state: achtung.Achtung, player_id: achtung.PlayerId) -> achtung.GameAction | None:
        action = random.choice(list(achtung.GameAction))
        return action


async def create_client(request_updates: bool):
    strat = RandomStrategy()
    client = await game_client.GameClient(
        game_state_type=achtung.Achtung,
        game_strategy=strat,
        request_updates=request_updates,
    ).connect("achtung.fly.dev", 443)
    # ).connect("0.0.0.0", 3030)
    await client.run()


async def run_clients() -> None:
    # run multiple clients concurrently
    tasks = []
    for i in range(8):
        tasks.append(asyncio.create_task(create_client(request_updates=i == 0)))

    await asyncio.gather(*tasks)


if __name__ == "__main__":
    asyncio.run(run_clients())
