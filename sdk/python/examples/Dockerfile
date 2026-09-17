# Example agent image. Build from the repo root so the SDK source is in context:
#
#   docker build -f sdk/python/examples/Dockerfile -t my-agent:local .
#
# The game host dials the agent on port 50052 inside its machine.
FROM python:3.12-slim

WORKDIR /app

COPY sdk/python /sdk
COPY sdk/python/examples/wall_avoider.py /app/agent.py

RUN pip install --no-cache-dir /sdk

EXPOSE 50052
ENV PORT=50052
ENV PYTHONUNBUFFERED=1

CMD ["python", "/app/agent.py"]
