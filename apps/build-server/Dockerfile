FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim

WORKDIR /app/build-server

# Install dependencies
ADD apps/build-server/pyproject.toml /app/build-server/pyproject.toml
ADD apps/build-server/uv.lock /app/build-server/uv.lock
RUN uv sync --frozen

# Copy the project into the image
ADD apps/build-server /app/build-server

# Run
ENTRYPOINT ["uv", "run", "python", "app.py"]
