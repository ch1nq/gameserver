import dataclasses
import logging
import subprocess
from typing import TypeAlias

import grpc
import httpx

import agent_deploy.protos.deploy_service_pb2 as deploy_service_pb2
import agent_deploy.protos.deploy_service_pb2_grpc as deploy_service_pb2_grpc


class DeployError(Exception): ...


FlySession: TypeAlias = httpx.Client
AgentId: TypeAlias = str
ImageUrl: TypeAlias = str
FlyOrg: TypeAlias = str
FlyAppName: TypeAlias = str


@dataclasses.dataclass
class AgentDeployJob:
    agent_id: AgentId
    image_url: ImageUrl


@dataclasses.dataclass
class RegistryCredentials:
    username: str
    password: str = dataclasses.field(repr=False)


def _create_fly_app(*, fly_session: FlySession, app_name: FlyAppName, fly_org: FlyOrg) -> None:
    """Create Fly.io app"""
    payload = {"app_name": app_name, "org_slug": fly_org}

    response = fly_session.post("https://api.machines.dev/v1/apps", json=payload)
    if response.status_code not in {201, 422}:  # 422 = already exists
        raise DeployError(f"Failed to create Fly app: {response.text}")


def _copy_image(
    *,
    source_image: ImageUrl,
    target_image: ImageUrl,
    target_crendentials: RegistryCredentials,
    source_credentials: RegistryCredentials | None = None,
) -> None:
    """Tag and push image to Fly registry"""
    src_creds = (
        [f"--src-creds={source_credentials.username}:{source_credentials.password}"]
        if source_credentials is not None
        else []
    )
    cmd = [
        "skopeo",
        "copy",
        *src_creds,
        f"--dest-creds={target_crendentials.username}:{target_crendentials.password}",
        f"docker://{source_image}",
        f"docker://{target_image}",
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        error = result.stderr.strip().replace(target_crendentials.password, "[redacted]")
        if source_credentials is not None:
            error = error.replace(source_credentials.password, "[redacted]")
        raise DeployError(f"failed to tag copy image: {error}")


def _create_machine(*, fly_session: FlySession, app_name: FlyAppName, image_url: ImageUrl) -> None:
    """Create a Fly machine"""
    url = f"https://api.machines.dev/v1/apps/{app_name}/machines"

    payload = {
        "config": {
            "image": image_url,
            "auto_destroy": False,
            "restart": {"policy": "never"},
        }
    }

    response = fly_session.post(url, json=payload)
    if response.status_code not in {200, 201}:
        raise DeployError(f"failed to create machine: {response.status_code} {response.text}")


@dataclasses.dataclass
class AgentDeployServiceConfig:
    fly_org: FlyOrg
    fly_api_token: str = dataclasses.field(repr=False)


class AgentDeployService(deploy_service_pb2_grpc.AgentDeployServiceServicer):
    def __init__(self, config: AgentDeployServiceConfig) -> None:
        super().__init__()
        self._config = config

        headers = {
            "Authorization": f"Bearer {config.fly_api_token}",
            "Content-Type": "application/json",
        }
        self._fly_session = httpx.Client(headers=headers, timeout=30.0)
        self._fly_registry_credentials = RegistryCredentials(
            username="x",
            password=config.fly_api_token,
        )

    def __del__(self) -> None:
        """Cleanup HTTP session on deletion"""
        self._fly_session.close()

    def DeployAgent(
        self,
        request: deploy_service_pb2.DeployAgentRequest,
        context: grpc.ServicerContext,
    ) -> deploy_service_pb2.DeployAgentResponse:
        app_name: FlyAppName = f"achtung-agent-{request.agent_id}"
        fly_image_url: ImageUrl = f"registry.fly.io/{app_name}:latest"

        # If source is not already in Fly registry, pull and push
        if request.image_url.startswith("registry.fly.io/"):
            return deploy_service_pb2.DeployAgentResponse(
                status=deploy_service_pb2.DeployAgentResponse.Status.ERROR,
                message="Source image cannot be in Fly registry",
            )

        try:
            logging.info(f"Creating Fly app: {app_name}")
            _create_fly_app(
                app_name=app_name,
                fly_org=self._config.fly_org,
                fly_session=self._fly_session,
            )

            logging.info(f"Copying image to Fly registry: {fly_image_url}")
            src_credentials = (
                RegistryCredentials(
                    username=request.registry_credentials.username,
                    password=request.registry_credentials.password,
                )
                if request.HasField("registry_credentials")
                else None
            )
            _copy_image(
                source_image=request.image_url,
                target_image=fly_image_url,
                source_credentials=src_credentials,
                target_crendentials=self._fly_registry_credentials,
            )

            logging.info(f"Creating machine for app: {app_name}")
            _create_machine(
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
