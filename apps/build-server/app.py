import logging
import os

from flask import Flask, jsonify, request
from kubernetes import client, config
from kubernetes.client.rest import ApiException

app = Flask(__name__)

# Load kubernetes configuration
config.load_incluster_config()

# Create kubernetes API clients
v1 = client.CoreV1Api()
apps_v1 = client.AppsV1Api()
batch_v1 = client.BatchV1Api()

_DOCKER_REGISTRY_NODE_PORT = os.getenv("DOCKER_REGISTRY_NODE_PORT", "30400")
_USER_CONTAINER_NAMESPACE = os.getenv("USER_CONTAINER_NAMESPACE", "default")


def create_kaniko_build_job(name: str, git_repo: str, dockerfile_path: str, context_sub_path: str) -> str:
    """
    Create a Kaniko build job to build and push the image to our local registry
    Returns the job name and the local image reference
    """
    local_image = f"docker-registry.registry-system.svc.cluster.local:5000/{name}:latest"

    job = {
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {"name": f"build-{name}", "namespace": "registry-system"},
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
                name=f"build-{name}",
                namespace="registry-system",
                body=client.V1DeleteOptions(propagation_policy="Background"),
            )
        except ApiException as e:
            if e.status != 404:  # Ignore if job doesn't exist
                raise

        # Create the new build job
        batch_v1.create_namespaced_job(body=job, namespace="registry-system")
        logging.info(f"Created build job for {name}")

        return f"build-{name}"

    except ApiException as e:
        logging.error(f"Failed to create build job: {e}")
        raise


def wait_for_job_completion(job_name: str, timeout_seconds: int = 300) -> bool:
    """Wait for the build job to complete"""
    import time

    start_time = time.time()

    while time.time() - start_time < timeout_seconds:
        try:
            job = batch_v1.read_namespaced_job_status(name=job_name, namespace="registry-system")
            if job.status.succeeded:
                return True
            if job.status.failed:
                raise Exception(f"Build job {job_name} failed")

            time.sleep(5)
        except ApiException as e:
            logging.error(f"Error checking job status: {e}")
            raise

    raise Exception(f"Build job {job_name} timed out after {timeout_seconds} seconds")


def create_or_update_deployment(name: str, image: str):
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
        logging.info(f"Created deployment '{name}'")
    except ApiException as e:
        if e.status == 409:
            logging.info(f"Deployment '{name}' exists, updating...")
            apps_v1.patch_namespaced_deployment(name=name, body=deployment, namespace=_USER_CONTAINER_NAMESPACE)
            logging.info(f"Updated deployment '{name}'")
        else:
            raise


@app.route("/deploy", methods=["POST"])
def deploy():
    """
    Endpoint to handle deployment requests
    Expected JSON payload:
    {
        "image": "image-name",
        "name": "app-name",
    }
    """

    try:
        data = request.json
        name = data["name"]
        image = data["image"]

        # Create/update the deployment with the new image
        create_or_update_deployment(name, image)

        return jsonify(
            {
                "status": "success",
                "message": f"Application {name} deployed successfully",
                "image": image,
            }
        ), 201

    except ApiException as e:
        logging.error(f"Kubernetes API error: {str(e)}")
        return jsonify({"status": "error", "message": f"Kubernetes API error: {str(e)}"}), e.status or 500
    except Exception as e:
        logging.error(f"Unexpected error: {str(e)}")
        return jsonify({"status": "error", "message": f"Unexpected error: {str(e)}"}), 500


@app.route("/build-and-deploy", methods=["POST"])
def build_and_deploy():
    """
    Endpoint to handle deployment requests
    Expected JSON payload:
    {
        "name": "app-name",
        "git_repo": "https://github.com/user/repo.git",
        "dockerfile_path": "Dockerfile",  # Optional
        "context_sub_path": ".", # Optional
    }
    """
    try:
        data = request.json
        name = data["name"]
        git_repo = data["git_repo"]
        dockerfile_path = data.get("dockerfile_path", "Dockerfile")
        context_sub_path = data.get("context_sub_path", ".")

        # Create and start the build job
        job_name = create_kaniko_build_job(
            name=name,
            git_repo=git_repo,
            dockerfile_path=dockerfile_path,
            context_sub_path=context_sub_path,
        )

        # Wait for the build to complete
        wait_for_job_completion(job_name)

        # Create/update the deployment with the new image
        image_name = f"localhost:{_DOCKER_REGISTRY_NODE_PORT}/{name}:latest"
        create_or_update_deployment(name, image_name)

        return jsonify(
            {
                "status": "success",
                "message": f"Application {name} built and deployed successfully",
                "image": image_name,
            }
        ), 201

    except ApiException as e:
        logging.error(f"Kubernetes API error: {str(e)}")
        return jsonify({"status": "error", "message": f"Kubernetes API error: {str(e)}"}), e.status or 500
    except Exception as e:
        logging.error(f"Unexpected error: {str(e)}")
        return jsonify({"status": "error", "message": f"Unexpected error: {str(e)}"}), 500


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    app.run(host="0.0.0.0", port=5000)
