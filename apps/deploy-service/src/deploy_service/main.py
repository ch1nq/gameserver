import asyncio
import logging
import os

import deploy
import grpc
import protos.deploy_service_pb2 as deploy_service_pb2
import protos.deploy_service_pb2_grpc as deploy_service_pb2_grpc


async def serve():
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )
    port = os.environ.get("PORT", "50051")

    config = deploy.AgentDeployServiceConfig(
        fly_org=os.environ.get("FLY_ORG", "achtung"),
        fly_api_token=os.environ["FLY_API_TOKEN"],
    )
    service = deploy.AgentDeployService(config=config)
    server = grpc.aio.server()

    deploy_service_pb2_grpc.add_AgentDeployServiceServicer_to_server(service, server)
    listen_addr = f"[::]:{port}"
    server.add_insecure_port(listen_addr)

    # Start background worker
    await service.start()

    await server.start()
    logging.info("Build service started on %s", listen_addr)

    try:
        await server.wait_for_termination()
    except KeyboardInterrupt:
        logging.info("Shutting down...")
        await service.shutdown()
        await server.stop(0)


def main():
    asyncio.run(serve())


if __name__ == "__main__":
    main()
