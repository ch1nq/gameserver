build app:
    docker build . -f apps/{{app}}/Dockerfile -t {{app}}:latest

compile-protos:
    uvx --from "grpcio-tools>=1.74,<1.75" python -m grpc_tools.protoc \
        -Iagent_deploy/protos=./protos \
        --python_out=apps/agent-deploy/src \
        --pyi_out=apps/agent-deploy/src \
        --grpc_python_out=apps/agent-deploy/src \
        protos/deploy_service.proto
