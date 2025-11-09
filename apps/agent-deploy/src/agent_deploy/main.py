import logging
import os
from concurrent import futures

import grpc

import agent_deploy.protos.deploy_service_pb2_grpc as deploy_service_pb2_grpc
import agent_deploy.service


def serve():
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )
    port = os.environ.get("PORT", "50051")

    config = agent_deploy.service.AgentDeployServiceConfig(
        fly_org=os.environ.get("FLY_ORG", "achtung"),
        fly_api_token=os.environ["FLY_API_TOKEN"],
    )
    service = agent_deploy.service.AgentDeployService(config=config)

    # Create a thread pool executor with max 10 workers
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))

    deploy_service_pb2_grpc.add_AgentDeployServiceServicer_to_server(service, server)
    listen_addr = f"[::]:{port}"
    server.add_insecure_port(listen_addr)

    server.start()
    logging.info("Build service started on %s", listen_addr)

    try:
        server.wait_for_termination()
    except KeyboardInterrupt:
        logging.info("Shutting down...")
        server.stop(10)


def main():
    serve()


if __name__ == "__main__":
    main()
