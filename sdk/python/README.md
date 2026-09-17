# Achtung Python agent SDK

Write a `Bot`, call `run`, package it as a container image. The host dials
your agent over gRPC (`Initialize` once, then the `Play` tick stream).

## Quickstart

Requires [uv](https://docs.astral.sh/uv/).

```bash
cd sdk/python
uv sync --group dev
uv run python scripts/codegen.py   # generate stubs from ../../protos (gitignored)
```

```python
from achtung import Action, Bot, GameState, run


class WallAvoider(Bot):
    def step(self, state: GameState) -> Action:
        me = state.me()
        if me.position.x < 100:
            return Action.LEFT
        return Action.STRAIGHT


run(WallAvoider())  # serves on PORT env, else 50052
```

`Bot` is a structural interface: subclassing it is optional — any object
with a `step(state) -> Action` method works.

See `examples/wall_avoider.py` for a runnable bot and `examples/Dockerfile`
for packaging (`docker build -f sdk/python/examples/Dockerfile -t my-agent .`
from the repo root).

## Slow bots

`step` runs off the event loop, so blocking compute is safe. While one `step`
call is still running, the server keeps answering every tick with the latest
finished action — slow decisions automatically mean acting every N ticks, with
no extra code. A `step` that raises or returns a non-`Action` never breaks the
stream; the previous action is held (the host eliminates broken streams but
tolerates stale answers). Until the first `step` finishes, the agent answers
`STRAIGHT`, matching the host's `Forward` default.

## Development

```bash
uv run python scripts/codegen.py  # regenerate stubs after proto changes
uv run ruff check .
uv run ruff format --check .
uv run ty check
uv run pytest
```
