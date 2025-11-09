import logging
import os
import random

import grpc

import agent_deploy.protos.deploy_service_pb2
import agent_deploy.protos.deploy_service_pb2_grpc
import agent_deploy.service


def test_deploy_request() -> None:
    config = agent_deploy.service.AgentDeployServiceConfig(
        fly_org=os.environ.get("FLY_ORG", "achtung"),
        fly_api_token=os.environ["FLY_API_TOKEN"],
    )

    agent_id = random.randint(1000, 9999)
    request = agent_deploy.protos.deploy_service_pb2.DeployAgentRequest(
        agent_id=agent_id,
        image_url="docker.io/library/busybox:latest",
    )

    service = agent_deploy.service.AgentDeployService(config=config)
    response = service.DeployAgent(request=request, context=None)

    logging.info(f"DeployAgent response: {response}")
    assert response.status == agent_deploy.protos.deploy_service_pb2.DeployAgentResponse.Status.SUCCESS
    logging.info(f"Deployed agent app: {response.app_name} with image: {response.deployed_image_url}")


def test_aganst_localhost():
    request = agent_deploy.protos.deploy_service_pb2.DeployAgentRequest(
        agent_id=random.randint(1000, 9999),
        image_url="docker.io/library/busybox:latest",
    )
    client = agent_deploy.protos.deploy_service_pb2_grpc.AgentDeployServiceStub(
        channel=grpc.insecure_channel("localhost:50051")
    )
    response = client.DeployAgent(request=request)
    logging.info(f"DeployAgent response: {response}")


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )
    # test_deploy_request()
    test_aganst_localhost()
