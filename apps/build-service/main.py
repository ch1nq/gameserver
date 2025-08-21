"""
Simplified Build Service - GitHub Actions + Fly.io integration
Pure async implementation using grpcio-async
"""

import asyncio
import logging
import os
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import TYPE_CHECKING

import grpc
from aiohttp import ClientError, ClientSession, ClientTimeout

if TYPE_CHECKING:
    from .protos import build_service_pb2, build_service_pb2_grpc
else:
    from protos import build_service_pb2, build_service_pb2_grpc


class BuildError(Exception):
    def __init__(self, message: str, status_code: int | None = None):
        super().__init__(message)
        self.message = message
        self.status_code = status_code


def _get_env(var: str) -> str:
    value = os.getenv(var)
    if not value:
        raise BuildError(f"Missing required environment variable: {var}")
    return value


@dataclass
class BuildJob:
    build_id: str
    user_id: str
    agent_name: str
    git_repo: str
    dockerfile_path: str | None = None
    context_sub_path: str | None = None
    status: build_service_pb2.PollBuildResponse.BuildStatus = build_service_pb2.PollBuildResponse.BuildStatus.RUNNING
    message: str = "Build queued"
    app_name: str | None = None
    workflow_run_id: str | None = None
    image_url: str | None = None
    error: str | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))

    def update_status(
        self, status: build_service_pb2.PollBuildResponse.BuildStatus, message: str, error: str | None = None
    ) -> None:
        self.status = status
        self.message = message
        self.error = error
        self.updated_at = datetime.now(timezone.utc)

    def is_terminal(self) -> bool:
        return self.status in {
            build_service_pb2.PollBuildResponse.BuildStatus.SUCCEEDED,
            build_service_pb2.PollBuildResponse.BuildStatus.FAILED,
        }

    def is_running(self) -> bool:
        return self.status == build_service_pb2.PollBuildResponse.BuildStatus.RUNNING


class BuildService(build_service_pb2_grpc.BuildServiceServicer):
    def __init__(self):
        # Config from environment
        self.github_token = _get_env("GITHUB_TOKEN")
        self.github_build_repo = _get_env("GITHUB_BUILD_REPO")
        self.fly_api_token = _get_env("FLY_API_TOKEN")
        self.fly_org = os.getenv("FLY_ORG", "achtung")

        # Job storage
        self._jobs: dict[str, BuildJob] = {}
        self._lock = asyncio.Lock()

        # HTTP clients
        self._github_session: ClientSession | None = None
        self._fly_session: ClientSession | None = None

        # Background task
        self._background_task: asyncio.Task | None = None

    async def start(self):
        """Start background worker"""
        self._background_task = asyncio.create_task(self._background_worker())

    async def _background_worker(self):
        """Background task for status polling and cleanup"""
        while True:
            try:
                # Poll running jobs
                async with self._lock:
                    jobs_to_poll = [job for job in self._jobs.values() if job.is_running() and job.workflow_run_id]

                for job in jobs_to_poll:
                    await self._update_job_status(job)

                # Cleanup old jobs
                await self._cleanup_old_jobs()

                await asyncio.sleep(10)
            except Exception as e:
                logging.error(f"Background worker error: {e}")
                await asyncio.sleep(30)

    async def _get_github_session(self) -> ClientSession:
        if self._github_session is None or self._github_session.closed:
            headers = {"Authorization": f"token {self.github_token}", "Accept": "application/vnd.github.v3+json"}
            self._github_session = ClientSession(headers=headers, timeout=ClientTimeout(total=30))
        return self._github_session

    async def _get_fly_session(self) -> ClientSession:
        if self._fly_session is None or self._fly_session.closed:
            headers = {"Authorization": f"Bearer {self.fly_api_token}", "Content-Type": "application/json"}
            self._fly_session = ClientSession(headers=headers, timeout=ClientTimeout(total=30))
        return self._fly_session

    async def _create_fly_app(self, app_name: str) -> None:
        """Create Fly.io app"""
        session = await self._get_fly_session()
        payload = {"app_name": app_name, "org_slug": self.fly_org}

        async with session.post("https://api.machines.dev/v1/apps", json=payload) as response:
            if response.status not in {201, 422}:  # 422 = already exists
                error_text = await response.text()
                raise BuildError(f"Failed to create Fly app: {error_text}")

    async def _trigger_github_build(self, job: BuildJob) -> None:
        """Trigger GitHub Actions build"""
        session = await self._get_github_session()

        payload = {
            "ref": "master",
            "inputs": {
                "user_repo": job.git_repo,
                "image_name": f"registry.fly.io/{job.app_name}",
                "dockerfile_path": job.dockerfile_path or "",
                "context_sub_path": job.context_sub_path or "",
            },
        }

        url = f"https://api.github.com/repos/{self.github_build_repo}/actions/workflows/build-agent.yml/dispatches"

        async with session.post(url, json=payload) as response:
            if response.status != 204:
                error_text = await response.text()
                raise BuildError(f"Failed to trigger GitHub build: {error_text}")

    async def _find_workflow_run(self, job: BuildJob) -> str | None:
        """Find workflow run ID for job"""
        session = await self._get_github_session()
        url = f"https://api.github.com/repos/{self.github_build_repo}/actions/runs?per_page=10"

        try:
            async with session.get(url) as response:
                if response.status == 200:
                    data = await response.json()
                    target_image = f"registry.fly.io/{job.app_name}"

                    for run in data.get("workflow_runs", []):
                        # Simple heuristic: find recent run for our app
                        if target_image in str(run.get("head_commit", {}).get("message", "")):
                            return str(run["id"])
                return None
        except Exception:
            return None

    async def _update_job_status(self, job: BuildJob) -> None:
        """Update job status from GitHub"""
        if not job.workflow_run_id:
            job.workflow_run_id = await self._find_workflow_run(job)
            if not job.workflow_run_id:
                return

        session = await self._get_github_session()
        url = f"https://api.github.com/repos/{self.github_build_repo}/actions/runs/{job.workflow_run_id}"

        try:
            async with session.get(url) as response:
                if response.status == 200:
                    data = await response.json()
                    github_status = data.get("status", "unknown")
                    conclusion = data.get("conclusion")

                    if github_status == "completed":
                        if conclusion == "success":
                            job.update_status(
                                build_service_pb2.PollBuildResponse.BuildStatus.SUCCEEDED,
                                "Build completed successfully",
                            )
                            job.image_url = f"registry.fly.io/{job.app_name}:latest"
                        else:
                            job.update_status(
                                build_service_pb2.PollBuildResponse.BuildStatus.FAILED, f"Build failed: {conclusion}"
                            )
        except Exception as e:
            job.update_status(build_service_pb2.PollBuildResponse.BuildStatus.FAILED, f"Failed to check status: {e}")

    async def _cleanup_old_jobs(self) -> None:
        """Remove jobs older than 1 hour"""
        async with self._lock:
            current_time = datetime.now(timezone.utc)
            old_jobs = [
                job_id
                for job_id, job in self._jobs.items()
                if (current_time - job.created_at).total_seconds() > 3600 and job.is_terminal()
            ]

            for job_id in old_jobs:
                del self._jobs[job_id]

    async def _run_build(self, job: BuildJob) -> None:
        """Execute complete build process"""
        try:
            # Create Fly app
            app_name = f"agent-{job.user_id}-{uuid.uuid4().hex[:8]}"
            job.app_name = app_name
            job.update_status(build_service_pb2.PollBuildResponse.BuildStatus.RUNNING, "Creating Fly app...")

            await self._create_fly_app(app_name)

            # Trigger GitHub build
            job.update_status(build_service_pb2.PollBuildResponse.BuildStatus.RUNNING, "Starting build...")
            await self._trigger_github_build(job)
            job.update_status(build_service_pb2.PollBuildResponse.BuildStatus.RUNNING, "Building via GitHub Actions...")

        except Exception as e:
            job.update_status(build_service_pb2.PollBuildResponse.BuildStatus.FAILED, str(e), str(e))

    def _extract_user_id(self, context: grpc.ServicerContext) -> str | None:
        metadata = dict(context.invocation_metadata())
        return metadata.get("user-id")

    def _validate_request(self, user_id: str, agent_name: str, git_repo: str) -> None:
        if not user_id or not agent_name or not git_repo:
            raise BuildError("User ID, agent name, and git repo are required")
        if len(git_repo.split("/")) != 2:
            raise BuildError("git_repo should be '<owner>/<repo>'")

    async def Build(
        self, request: build_service_pb2.BuildRequest, context: grpc.aio.ServicerContext
    ) -> build_service_pb2.BuildResponse:
        try:
            user_id = self._extract_user_id(context)
            if not user_id:
                await context.set_code(grpc.StatusCode.UNAUTHENTICATED)
                return build_service_pb2.BuildResponse(
                    status=build_service_pb2.BuildResponse.Status.ERROR, message="Authentication required"
                )

            self._validate_request(user_id, request.name, request.git_repo)

            # Check rate limit
            async with self._lock:
                user_running_jobs = sum(1 for job in self._jobs.values() if job.user_id == user_id and job.is_running())
                if user_running_jobs >= 2:
                    raise BuildError("Too many concurrent builds")

                # Create job
                build_id = f"build-{uuid.uuid4().hex[:8]}"
                job = BuildJob(
                    build_id=build_id,
                    user_id=user_id,
                    agent_name=request.name,
                    git_repo=request.git_repo,
                    dockerfile_path=request.dockerfile_path or None,
                    context_sub_path=request.context_sub_path or None,
                )

                self._jobs[build_id] = job

            # Start build in background (no threading needed!)
            asyncio.create_task(self._run_build(job))

            return build_service_pb2.BuildResponse(
                status=build_service_pb2.BuildResponse.Status.SUCCESS,
                message=f"Building agent '{request.name}'",
                build_id=build_id,
            )

        except BuildError as e:
            await context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
            return build_service_pb2.BuildResponse(
                status=build_service_pb2.BuildResponse.Status.ERROR, message=e.message
            )
        except Exception as e:
            await context.set_code(grpc.StatusCode.INTERNAL)
            return build_service_pb2.BuildResponse(
                status=build_service_pb2.BuildResponse.Status.ERROR, message="Internal server error"
            )

    async def PollBuild(
        self, request: build_service_pb2.PollBuildRequest, context: grpc.aio.ServicerContext
    ) -> build_service_pb2.PollBuildResponse:
        async with self._lock:
            job = self._jobs.get(request.build_id)

        if not job:
            return build_service_pb2.PollBuildResponse(
                status=build_service_pb2.PollBuildResponse.Status.ERROR,
                message="Build job not found",
                build_status=build_service_pb2.PollBuildResponse.BuildStatus.UNKNOWN,
            )

        return build_service_pb2.PollBuildResponse(
            status=build_service_pb2.PollBuildResponse.Status.SUCCESS, message=job.message, build_status=job.status
        )

    async def Deploy(
        self, request: build_service_pb2.DeployRequest, context: grpc.aio.ServicerContext
    ) -> build_service_pb2.DeployResponse:
        try:
            user_id = self._extract_user_id(context)
            if not user_id:
                await context.set_code(grpc.StatusCode.UNAUTHENTICATED)
                return build_service_pb2.DeployResponse(
                    status=build_service_pb2.DeployResponse.Status.ERROR, message="Authentication required"
                )

            # Find latest successful build
            async with self._lock:
                latest_job = None
                for job in self._jobs.values():
                    if (
                        job.user_id == user_id
                        and job.agent_name == request.name
                        and job.status == build_service_pb2.PollBuildResponse.BuildStatus.SUCCEEDED
                        and job.image_url
                    ):
                        if not latest_job or job.created_at > latest_job.created_at:
                            latest_job = job

            if not latest_job:
                return build_service_pb2.DeployResponse(
                    status=build_service_pb2.DeployResponse.Status.ERROR,
                    message=f"No successful build found for agent '{request.name}'",
                )

            return build_service_pb2.DeployResponse(
                status=build_service_pb2.DeployResponse.Status.SUCCESS, message=f"Agent ready: {latest_job.image_url}"
            )

        except Exception as e:
            await context.set_code(grpc.StatusCode.INTERNAL)
            return build_service_pb2.DeployResponse(
                status=build_service_pb2.DeployResponse.Status.ERROR, message="Failed to get deployment info"
            )

    async def shutdown(self):
        if self._background_task:
            self._background_task.cancel()
        if self._github_session and not self._github_session.closed:
            await self._github_session.close()
        if self._fly_session and not self._fly_session.closed:
            await self._fly_session.close()


async def serve():
    logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s")

    server = grpc.aio.server()
    service = BuildService()

    build_service_pb2_grpc.add_BuildServiceServicer_to_server(service, server)
    listen_addr = "[::]:50051"
    server.add_insecure_port(listen_addr)

    # Start background worker
    await service.start()

    await server.start()
    logging.info("Build service started on port 50051")

    try:
        await server.wait_for_termination()
    except KeyboardInterrupt:
        logging.info("Shutting down...")
        await service.shutdown()
        await server.stop(0)


if __name__ == "__main__":
    asyncio.run(serve())
