FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim

WORKDIR /app/build-service

# Install dependencies
ADD apps/build-service/pyproject.toml /app/build-service/pyproject.toml
ADD apps/build-service/uv.lock /app/build-service/uv.lock
RUN uv sync --frozen

# Copy the project into the image
ADD apps/build-service /app/build-service

# Run
ENTRYPOINT ["uv", "run", "python", "main.py"]
