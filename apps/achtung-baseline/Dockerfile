FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim AS builder

WORKDIR /app/achtung-baseline

# Copy the project into the image
ADD apps/achtung-baseline/pyproject.toml /app/achtung-baseline/pyproject.toml
ADD apps/achtung-baseline/uv.lock /app/achtung-baseline/uv.lock
ADD libs/arcadio-client /libs/arcadio-client

RUN uv sync --frozen


FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim AS runner

WORKDIR /app/achtung-baseline

ADD libs/arcadio-client /libs/arcadio-client
ADD apps/achtung-baseline /app/achtung-baseline
COPY --from=builder /app /app

ENTRYPOINT ["uv", "run", "python", "main.py"]
