FROM python:3.12-slim
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

WORKDIR /app/achtung-baseline

# Install dependencies
ADD libs/arcadio-client /libs/arcadio-client
COPY apps/achtung-baseline/uv.lock apps/achtung-baseline/pyproject.toml /app/achtung-baseline/
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-dev --no-install-project --no-editable

# Copy the project into the image
ADD apps/achtung-baseline/ /app/achtung-baseline
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --frozen --no-dev --no-editable

ENTRYPOINT ["/app/achtung-baseline/.venv/bin/python", "main.py"]
