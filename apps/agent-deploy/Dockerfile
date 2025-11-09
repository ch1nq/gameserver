FROM python:3.12-slim AS builder
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

WORKDIR /app

# Install dependencies
COPY apps/agent-deploy/pyproject.toml apps/agent-deploy/uv.lock ./
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --locked --no-install-project --no-editable

COPY apps/agent-deploy ./
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --locked --no-editable

FROM python:3.12-slim
# Install skopeo
RUN apt-get update && apt-get install -y --no-install-recommends \
    skopeo \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder --chown=app:app /app/.venv /app/.venv

CMD ["/app/.venv/bin/serve"]
