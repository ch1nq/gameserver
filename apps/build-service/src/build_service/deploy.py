import asyncio
import logging
from typing import Optional

from aiohttp import ClientSession, ClientTimeout

from .errors import BuildError


class FlyDeployer:
    def __init__(self, fly_api_token: str, fly_org: str = "achtung") -> None:
        self.fly_api_token = fly_api_token
        self.fly_org = fly_org
        self._fly_session: Optional[ClientSession] = None

    async def _get_fly_session(self) -> ClientSession:
        if self._fly_session is None or self._fly_session.closed:
            headers = {"Authorization": f"Bearer {self.fly_api_token}", "Content-Type": "application/json"}
            self._fly_session = ClientSession(headers=headers, timeout=ClientTimeout(total=30))
        return self._fly_session

    async def create_app(self, app_name: str) -> None:
        session = await self._get_fly_session()
        payload = {"app_name": app_name, "org_slug": self.fly_org}

        async with session.post("https://api.machines.dev/v1/apps", json=payload) as response:
            if response.status not in {201, 422}:  # 422 = already exists
                error_text = await response.text()
                raise BuildError(f"Failed to create Fly app: {error_text}")

    async def create_machine(self, app_name: str, image_url: str) -> None:
        session = await self._get_fly_session()
        url = f"https://api.machines.dev/v1/apps/{app_name}/machines"

        payload = {
            "config": {
                "image": image_url,
                "auto_destroy": False,
                "restart": {"policy": "always"},
            }
        }

        async with session.post(url, json=payload) as response:
            if response.status not in {200, 201}:
                error_text = await response.text()
                raise BuildError(f"Failed to create machine: {error_text}")

    async def list_machines(self, app_name: str) -> list[dict]:
        session = await self._get_fly_session()
        url = f"https://api.machines.dev/v1/apps/{app_name}/machines"

        try:
            async with session.get(url) as response:
                if response.status == 200:
                    return await response.json()
                return []
        except Exception:
            return []

    async def delete_machine(self, app_name: str, machine_id: str) -> None:
        session = await self._get_fly_session()
        url = f"https://api.machines.dev/v1/apps/{app_name}/machines/{machine_id}"

        async with session.delete(url) as response:
            if response.status not in {200, 204}:
                error_text = await response.text()
                logging.warning(f"Failed to delete machine {machine_id}: {error_text}")

    async def pull_docker_image(self, image_url: str) -> None:
        proc = await asyncio.create_subprocess_exec(
            "docker", "pull", image_url, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        _, stderr = await proc.communicate()

        if proc.returncode != 0:
            raise BuildError(f"Failed to pull image: {stderr.decode()}")

    async def tag_and_push_image(self, source_image: str, target_image: str) -> None:
        proc = await asyncio.create_subprocess_exec(
            "docker", "tag", source_image, target_image, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        _, stderr = await proc.communicate()

        if proc.returncode != 0:
            raise BuildError(f"Failed to tag image: {stderr.decode()}")

        proc = await asyncio.create_subprocess_exec(
            "docker", "push", target_image, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )
        _, stderr = await proc.communicate()

        if proc.returncode != 0:
            raise BuildError(f"Failed to push image: {stderr.decode()}")

    async def shutdown(self) -> None:
        if self._fly_session and not self._fly_session.closed:
            await self._fly_session.close()
