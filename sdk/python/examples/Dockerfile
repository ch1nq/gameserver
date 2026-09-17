# Example agent image, following https://docs.astral.sh/uv/guides/integration/docker/.
# Build from the repo root so the SDK source is in context:
#
#   docker build -f sdk/python/examples/Dockerfile -t my-agent:local .
#
# The game host dials the agent on port 50052 inside its machine.
FROM python:3.12-slim-trixie
COPY --from=ghcr.io/astral-sh/uv:0.12.15 /uv /uvx /bin/

# Production niceties from the uv Docker guide.
ENV UV_COMPILE_BYTECODE=1
ENV UV_LINK_MODE=copy

# The SDK's dev dependencies provide grpcio-tools, which generates the gRPC
# stubs from protos/ at build time (generated code is never checked in).
WORKDIR /sdk

# Install dependencies first (cached layer): needs only the manifests.
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=sdk/python/uv.lock,target=uv.lock \
    --mount=type=bind,source=sdk/python/pyproject.toml,target=pyproject.toml \
    uv sync --locked --no-install-project

# Then sources plus stub generation.
COPY sdk/python /sdk
COPY protos /protos
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --locked \
 && uv run python scripts/codegen.py

# Run the agent from the project environment.
ENV PATH="/sdk/.venv/bin:$PATH"
WORKDIR /app
COPY sdk/python/examples/wall_avoider.py /app/agent.py

EXPOSE 50052
ENV PORT=50052
ENV PYTHONUNBUFFERED=1

CMD ["python", "/app/agent.py"]
