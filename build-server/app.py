import logging

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


def get_docker_registry_port() -> int:
    """Get the port of the local docker registry"""
    try:
        # Get the registry service
        service = v1.read_namespaced_service(
            name="docker-registry", namespace="registry-system", async_req=False
        )
        logging.info(f"Got registry service: {service}")
        return service.spec.ports[0].node_port
    except ApiException as e:
        logging.error(f"Failed to get registry service: {e}")
        raise


def create_kaniko_build_job(
    name: str,
    git_repo: str,
    dockerfile_path: str,
    context_sub_path: str,
) -> tuple[str, str]:
    """
    Create a Kaniko build job to build and push the image to our local registry
    Returns the job name and the local image reference
    """
    local_image = (
        f"docker-registry.registry-system.svc.cluster.local:5000/{name}:latest"
    )

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

        return f"build-{name}", local_image

    except ApiException as e:
        logging.error(f"Failed to create build job: {e}")
        raise


def wait_for_job_completion(job_name: str, timeout_seconds: int = 300) -> bool:
    """Wait for the build job to complete"""
    import time

    start_time = time.time()

    while time.time() - start_time < timeout_seconds:
        try:
            job = batch_v1.read_namespaced_job_status(
                name=job_name, namespace="registry-system"
            )
            if job.status.succeeded:
                return True
            if job.status.failed:
                raise Exception(f"Build job {job_name} failed")

            time.sleep(5)
        except ApiException as e:
            logging.error(f"Error checking job status: {e}")
            raise

    raise Exception(f"Build job {job_name} timed out after {timeout_seconds} seconds")


def create_or_update_deployment(name: str, image: str, port: int = 80):
    """Create or update a kubernetes deployment and service"""
    # Deployment template
    deployment = {
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {"name": name},
        "spec": {
            "replicas": 1,
            "selector": {"matchLabels": {"app": name}},
            "template": {
                "metadata": {"labels": {"app": name}},
                "spec": {
                    "containers": [
                        {
                            "name": name,
                            "image": image,
                            "ports": [{"containerPort": port}],
                            "imagePullPolicy": "Always",  # Important for local registry
                        }
                    ]
                },
            },
        },
    }

    # Service template
    service = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": f"{name}-service"},
        "spec": {
            "selector": {"app": name},
            "ports": [{"port": port, "targetPort": port}],
            "type": "ClusterIP",
        },
    }

    try:
        # Try to create deployment
        apps_v1.create_namespaced_deployment(body=deployment, namespace="default")
        logging.info(f"Created deployment {name}")
    except ApiException as e:
        if e.status == 409:  # Already exists
            # Update the existing deployment
            logging.info(f"Deployment {name} exists, updating...")
            apps_v1.patch_namespaced_deployment(
                name=name, namespace="default", body=deployment
            )
            logging.info(f"Updated deployment {name}")
        else:
            raise

    try:
        # Try to create service
        v1.create_namespaced_service(body=service, namespace="default")
        logging.info(f"Created service {name}-service")
    except ApiException as e:
        if e.status == 409:  # Already exists
            # Update the existing service
            logging.info(f"Service {name}-service exists, updating...")
            v1.patch_namespaced_service(
                name=f"{name}-service", namespace="default", body=service
            )
            logging.info(f"Updated service {name}-service")
        else:
            raise


@app.route("/deploy", methods=["POST"])
def deploy():
    """
    Endpoint to handle deployment requests
    Expected JSON payload:
    {
        "name": "app-name",
        "git_repo": "https://github.com/user/repo.git",
        "dockerfile_path": "Dockerfile",  # Optional
        "context_sub_path": ".", # Optional
        "port": 80  # Optional
    }
    """
    try:
        data = request.json
        name = data["name"]
        git_repo = data["git_repo"]
        dockerfile_path = data.get("dockerfile_path", "Dockerfile")
        context_sub_path = data.get("context_sub_path", ".")
        port = data.get("port", 80)

        # Create and start the build job
        job_name, _ = create_kaniko_build_job(
            name=name,
            git_repo=git_repo,
            dockerfile_path=dockerfile_path,
            context_sub_path=context_sub_path,
        )

        registry_port = get_docker_registry_port()
        image_name = f"localhost:{registry_port}/{name}:latest"

        # Wait for the build to complete
        wait_for_job_completion(job_name)

        # Create/update the deployment with the new image
        create_or_update_deployment(name, image_name, port)

        return jsonify(
            {
                "status": "success",
                "message": f"Application {name} built and deployed successfully",
                "image": image_name,
            }
        ), 201

    except ApiException as e:
        logging.error(f"Kubernetes API error: {str(e)}")
        return jsonify(
            {"status": "error", "message": f"Kubernetes API error: {str(e)}"}
        ), e.status or 500
    except Exception as e:
        logging.error(f"Unexpected error: {str(e)}")
        return jsonify(
            {"status": "error", "message": f"Unexpected error: {str(e)}"}
        ), 500


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    app.run(host="0.0.0.0", port=5000)
