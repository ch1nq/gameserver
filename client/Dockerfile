# Use the official lightweight Python image.
# https://hub.docker.com/_/python
FROM python:3.12-slim

# RUN apt-get update && apt-get install -y --no-install-recommends curl
# RUN curl -sSL https://install.python-poetry.org | python3 - 
RUN pip install poetry

# Copy local code to the container image.
ENV APP_HOME /app
WORKDIR $APP_HOME
COPY poetry.lock .
COPY pyproject.toml .
COPY . .

# Install production dependencies.
RUN poetry install --only main

# Run 
ENTRYPOINT ["poetry", "run", "python", "client.py"]

