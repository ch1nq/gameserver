FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim

WORKDIR /app

# Install dependencies
ADD pyproject.toml /app/pyproject.toml
ADD uv.lock /app/uv.lock
RUN uv sync --frozen

# Copy the project into the image
ADD . /app


# Run
ENTRYPOINT ["uv", "run", "python", "app.py"]
