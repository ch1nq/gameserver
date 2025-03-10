import logging
import os
import time
import uuid
from concurrent import futures

import grpc
from kubernetes import client, config
from kubernetes.client.rest import ApiException

# Import generated protobuf code
from protos import build_service_pb2, build_service_pb2_grpc

_DOCKER_REGISTRY_NODE_PORT = os.getenv("DOCKER_REGISTRY_NODE_PORT", "30400")
_USER_CONTAINER_NAMESPACE = os.getenv("USER_CONTAINER_NAMESPACE", "default")


def create_kaniko_build_job(
    *,
    name: str,
    git_repo: str,
    dockerfile_path: str,
    context_sub_path: str,
    batch_v1: client.BatchV1Api,
    core_client: client.CoreV1Api,
) -> str:
    """
    Create a Kaniko build job to build and push the image to our local registry
    Returns the job name
    """
    local_image = f"docker-registry.registry-system.svc.cluster.local:5000/{name}:latest"
    build_id = f"build.{name}.{uuid.uuid4().hex}"

    job = {
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {"name": build_id, "namespace": "registry-system"},
        "spec": {
            "backoffLimit": 0,  # Don't retry on failure
            "ttlSecondsAfterFinished": 3600,  # Clean up after 1 hour
            "template": {
                "spec": {
                    "containers": [
                        {
                            "name": "kaniko",
                            "image": "gcr.io/kaniko-project/executor:latest",
                            "args": [
                                f"--dockerfile={dockerfile_path}",
                                f"--context=git://{git_repo}",
                                f"--context-sub-path={context_sub_path}",
                                f"--destination={local_image}",
                                "--insecure",
                                "--skip-tls-verify",
                            ],
                        }
                    ],
                    "restartPolicy": "Never",
                    "serviceAccountName": "kaniko-builder",
                }
            },
        },
    }

    try:
        # Delete any existing job with the same name
        try:
            batch_v1.delete_namespaced_job(
                name=build_id,
                namespace="registry-system",
                body=client.V1DeleteOptions(propagation_policy="Background"),
            )
        except ApiException as e:
            if e.status != 404:  # Ignore if job doesn't exist
                raise

        # Create the new build job
        batch_v1.create_namespaced_job(body=job, namespace="registry-system")
        logging.info("Created build job for %s", name)

        return build_id

    except ApiException as e:
        logging.error("Failed to create build job: %s", e)
        raise


def create_or_update_deployment(*, name: str, image: str, apps_v1: client.AppsV1Api):
    """Create or update a kubernetes deployment"""

    name = f"gameclient-{name}"
    deployment = client.V1Deployment(
        api_version="apps/v1",
        kind="Deployment",
        metadata=client.V1ObjectMeta(name=name),
        spec=client.V1DeploymentSpec(
            replicas=8,
            selector=client.V1LabelSelector(match_labels={"app": name}),
            template=client.V1PodTemplateSpec(
                metadata=client.V1ObjectMeta(labels={"app": name, "is-gameclient": "true"}),
                spec=client.V1PodSpec(
                    containers=[
                        client.V1Container(
                            name=name,
                            image=image,
                            image_pull_policy="Always",
                            env=[
                                client.V1EnvVar(name="SERVER_HOST", value="gameserver.default.svc.cluster.local"),
                                client.V1EnvVar(name="SERVER_PORT", value="80"),
                            ],
                            resources=client.V1ResourceRequirements(
                                requests={"cpu": "100m", "memory": "128Mi"},
                                limits={"cpu": "250m", "memory": "256Mi"},
                            ),
                        )
                    ]
                ),
            ),
        ),
    )

    # Try to create deployment and update if it already exists
    try:
        apps_v1.create_namespaced_deployment(body=deployment, namespace=_USER_CONTAINER_NAMESPACE)
        logging.info("Created deployment '%s'", name)
    except ApiException as e:
        if e.status == 409:
            logging.info("Deployment '%s' exists, updating...", name)
            apps_v1.patch_namespaced_deployment(name=name, body=deployment, namespace=_USER_CONTAINER_NAMESPACE)
            logging.info("Updated deployment '%s'", name)
        else:
            raise


class BuildServiceServicer(build_service_pb2_grpc.BuildServiceServicer):
    """Implementation of the BuildService gRPC service"""

    def __init__(self):
        # Load kubernetes configuration
        config.load_incluster_config()

        # Create kubernetes API clients
        self.core_v1 = client.CoreV1Api()
        self.apps_v1 = client.AppsV1Api()
        self.batch_v1 = client.BatchV1Api()

    def Build(
        self, request: build_service_pb2.BuildRequest, context: grpc.ServicerContext
    ) -> build_service_pb2.BuildResponse:
        """Handle build requests"""
        try:
            logging.info("Building %s from %s", request.name, request.git_repo)

            # Set default values if not provided
            dockerfile_path = request.dockerfile_path or "Dockerfile"
            context_sub_path = request.context_sub_path or "."

            # Create the build job
            build_id = create_kaniko_build_job(
                name=request.name,
                git_repo=request.git_repo,
                dockerfile_path=dockerfile_path,
                context_sub_path=context_sub_path,
                batch_v1=self.batch_v1,
                core_client=self.core_v1,
            )

            # Return success response
            return build_service_pb2.BuildResponse(
                status=build_service_pb2.BuildResponse.Status.SUCCESS,
                message=f"Started build job for '{request.name}'",
                build_id=build_id,
            )

        except Exception as e:
            logging.error("Build error: %s", str(e))
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(e))
            return build_service_pb2.BuildResponse(
                status=build_service_pb2.BuildResponse.Status.ERROR, message=f"Build failed: {str(e)}"
            )

    def PollBuild(
        self, request: build_service_pb2.PollBuildRequest, context: grpc.ServicerContext
    ) -> build_service_pb2.PollBuildResponse:
        """Poll the status of a build job"""
        try:
            # Get the job status
            job = self.batch_v1.read_namespaced_job_status(name=request.build_id, namespace="registry-system")

            logging.info("Build job %s status: %s", request.build_id, job.status)

            # Determine the build status
            build_status = build_service_pb2.PollBuildResponse.BuildStatus.UNKNOWN
            message = "Build job status unknown"

            if job.status.active:
                build_status = build_service_pb2.PollBuildResponse.BuildStatus.RUNNING
                message = "Build job is running"
            elif job.status.succeeded:
                build_status = build_service_pb2.PollBuildResponse.BuildStatus.SUCCEEDED
                message = "Build job succeeded"
            elif job.status.failed:
                build_status = build_service_pb2.PollBuildResponse.BuildStatus.FAILED
                message = "Build job failed"

            # Return the response
            return build_service_pb2.PollBuildResponse(
                status=build_service_pb2.PollBuildResponse.Status.SUCCESS, message=message, build_status=build_status
            )

        except ApiException as e:
            logging.error("Poll build error: %s", str(e))
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(e))
            return build_service_pb2.PollBuildResponse(
                status=build_service_pb2.PollBuildResponse.Status.ERROR,
                message=f"Failed to poll build: {str(e)}",
                build_status=build_service_pb2.PollBuildResponse.BuildStatus.UNKNOWN,
            )

        except Exception as e:
            logging.error("Unexpected poll build error: %s", str(e))
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(e))
            return build_service_pb2.PollBuildResponse(
                status=build_service_pb2.PollBuildResponse.Status.ERROR,
                message=f"Unexpected error: {str(e)}",
                build_status=build_service_pb2.PollBuildResponse.BuildStatus.UNKNOWN,
            )

    def Deploy(
        self, request: build_service_pb2.DeployRequest, context: grpc.ServicerContext
    ) -> build_service_pb2.DeployResponse:
        """Deploy a built container"""
        try:
            # Create/update the deployment with the new image
            image_name = f"localhost:{_DOCKER_REGISTRY_NODE_PORT}/{request.name}:latest"
            create_or_update_deployment(name=request.name, image=image_name, apps_v1=self.apps_v1)

            # Return success response
            return build_service_pb2.DeployResponse(
                status=build_service_pb2.DeployResponse.Status.SUCCESS,
                message=f"Application {request.name} deployed successfully",
            )

        except ApiException as e:
            logging.error("Deploy error: %s", str(e))
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(e))
            return build_service_pb2.DeployResponse(
                status=build_service_pb2.DeployResponse.Status.ERROR, message=f"Kubernetes API error: {str(e)}"
            )

        except Exception as e:
            logging.error("Unexpected deploy error: %s", str(e))
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(e))
            return build_service_pb2.DeployResponse(
                status=build_service_pb2.DeployResponse.Status.ERROR, message=f"Unexpected error: {str(e)}"
            )


def serve():
    """Start the gRPC server"""
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
    build_service_pb2_grpc.add_BuildServiceServicer_to_server(BuildServiceServicer(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    logging.info("Build service server started on port 50051")
    try:
        while True:
            time.sleep(86400)  # One day in seconds
    except KeyboardInterrupt:
        server.stop(0)


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    serve()
