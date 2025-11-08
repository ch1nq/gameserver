import asyncio
import dataclasses
import logging

import grpc
import protos.deploy_service_pb2 as deploy_service_pb2
import protos.deploy_service_pb2_grpc as deploy_service_pb2_grpc
from aiohttp import ClientSession, ClientTimeout


class DeployError(Exception): ...


async def _pull_docker_image(*, image_url: str) -> None:
    """Pull image from registry using Docker"""
    proc = await asyncio.create_subprocess_exec(
        "docker",
        "pull",
        image_url,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    _, stderr = await proc.communicate()

    if proc.returncode != 0:
        raise DeployError("failed to pull docker image:" + stderr.decode().strip())


async def _create_fly_app(*, fly_session: ClientSession, app_name: str, fly_org: str) -> None:
    """Create Fly.io app"""
    payload = {"app_name": app_name, "org_slug": fly_org}

    async with fly_session.post("https://api.machines.dev/v1/apps", json=payload) as response:
        if response.status not in {201, 422}:  # 422 = already exists
            error_text = await response.text()
            raise DeployError(f"Failed to create Fly app: {error_text}")


async def _tag_and_push_image(*, source_image: str, target_image: str) -> None:
    """Tag and push image to Fly registry"""
    # Tag
    proc = await asyncio.create_subprocess_exec(
        "docker",
        "tag",
        source_image,
        target_image,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    _, stderr = await proc.communicate()

    if proc.returncode != 0:
        raise DeployError("failed to tag docker image:" + stderr.decode().strip())

    # Push
    proc = await asyncio.create_subprocess_exec(
        "docker",
        "push",
        target_image,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    _, stderr = await proc.communicate()

    if proc.returncode != 0:
        raise DeployError("failed to push docker image:" + stderr.decode().strip())


async def _create_machine(*, fly_session: ClientSession, app_name: str, image_url: str) -> None:
    """Create a Fly machine"""
    url = f"https://api.machines.dev/v1/apps/{app_name}/machines"

    payload = {
        "config": {
            "image": image_url,
            "auto_destroy": False,
            "restart": {"policy": "always"},
        }
    }

    async with fly_session.post(url, json=payload) as response:
        if response.status not in {200, 201}:
            error_text = await response.text()
            raise DeployError(f"failed to create machine: {response.status} {error_text}")


@dataclasses.dataclass
class AgentDeployServiceConfig:
    fly_org: str
    fly_api_token: str = dataclasses.field(repr=False)


class AgentDeployService(deploy_service_pb2_grpc.AgentDeployServiceServicer):
    def __init__(self, config: AgentDeployServiceConfig) -> None:
        super().__init__()
        self._config = config

        headers = {
            "Authorization": f"Bearer {config.fly_api_token}",
            "Content-Type": "application/json",
        }
        self._fly_session = ClientSession(headers=headers, timeout=ClientTimeout(total=30))

    async def DeployAgent(
        self,
        request: deploy_service_pb2.DeployAgentRequest,
        context: grpc.ServicerContext,
    ) -> deploy_service_pb2.DeployAgentResponse:
        app_name = f"agent-{request.agent_id}"
        fly_image_url = f"registry.fly.io/{app_name}:latest"

        # If source is not already in Fly registry, pull and push
        if request.image_url.startswith("registry.fly.io/"):
            return deploy_service_pb2.DeployAgentResponse(
                status=deploy_service_pb2.DeployAgentResponse.Status.ERROR,
                message="Source image cannot be in Fly registry",
            )

        try:
            logging.info(f"Pulling image from external registry: {request.image_url}")
            await _pull_docker_image(image_url=request.image_url)

            logging.info(f"Pushing to Fly registry: {fly_image_url}")
            await _tag_and_push_image(source_image=request.image_url, target_image=fly_image_url)

            logging.info(f"Creating Fly app: {app_name}")
            await _create_fly_app(
                app_name=app_name,
                fly_org=self._config.fly_org,
                fly_session=self._fly_session,
            )

            logging.info(f"Creating machine for app: {app_name}")
            await _create_machine(
                app_name=app_name,
                image_url=fly_image_url,
                fly_session=self._fly_session,
            )
        except DeployError as error:
            return deploy_service_pb2.DeployAgentResponse(
                status=deploy_service_pb2.DeployAgentResponse.Status.ERROR,
                message=str(error),
            )

        return deploy_service_pb2.DeployAgentResponse(
            status=deploy_service_pb2.DeployAgentResponse.Status.SUCCESS,
            app_name=app_name,
            deployed_image_url=fly_image_url,
        )

    def DeleteAgent(
        self,
        request: deploy_service_pb2.DeleteAgentRequest,
        context: grpc.ServicerContext,
    ) -> deploy_service_pb2.DeleteAgentResponse:
        """Missing associated documentation comment in .proto file."""
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details("Method not implemented!")
        raise NotImplementedError("Method not implemented!")
