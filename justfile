build app:
    docker build . -f apps/{{app}}/Dockerfile -t {{app}}:latest

compile-protos:
    uvx --from "grpcio-tools>=1.74,<1.75" python -m grpc_tools.protoc \
        -Iagent_deploy/protos=./protos \
        --python_out=apps/agent-deploy/src \
        --pyi_out=apps/agent-deploy/src \
        --grpc_python_out=apps/agent-deploy/src \
        protos/deploy_service.proto

# Build the game host and load it into the microsandbox image cache.
# msb keeps its own image cache and cannot see the Docker daemon's images,
# so a locally-built tag has to be handed over explicitly via `docker save | msb load`.
# The cache lives in the `microsandbox_data` volume, so this needs the website
# container running. Default tag must match COORDINATOR__GAME_HOST_IMAGE in docker-compose.yml.
load-game-host tag="achtung-game-host:local":
    docker build . -f apps/achtung-host/Dockerfile -t {{tag}}
    docker save {{tag}} | docker compose exec -T website /root/.microsandbox/bin/msb load
