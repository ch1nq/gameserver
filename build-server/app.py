# app.py
from kubernetes import client, config
from flask import Flask, request, jsonify
import os
import docker
import logging
from datetime import datetime

app = Flask(__name__)
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# Load kubernetes configuration - works both in and out of cluster
try:
    config.load_incluster_config()
except config.ConfigException:
    config.load_kube_config()

k8s_apps_v1 = client.AppsV1Api()
k8s_core_v1 = client.CoreV1Api()
docker_client = docker.from_env()

def validate_deployment_request(data):
    """Validate incoming deployment request data"""
    required_fields = ['name', 'image']
    
    if not all(field in data for field in required_fields):
        raise ValueError(f"Missing required fields. Required: {required_fields}")
    
    # Validate name format (lowercase, alphanumeric with dashes)
    if not data['name'].replace('-', '').isalnum():
        raise ValueError("Name must be alphanumeric with optional dashes")
    
    # Basic image name validation
    if ':' not in data['image']:
        raise ValueError("Image must include a tag (e.g., nginx:latest)")

@app.route('/health', methods=['GET'])
def health_check():
    """Health check endpoint"""
    return jsonify({"status": "healthy", "timestamp": datetime.utcnow().isoformat()})

@app.route('/deploy', methods=['POST'])
def deploy_container():
    """Handle container deployment requests"""
    try:
        data = request.get_json()
        if not data:
            raise ValueError("No JSON data received")

        logger.info(f"Received deployment request for: {data.get('name', 'unknown')}")
        
        # Validate request data
        validate_deployment_request(data)
        
        image_name = data['image']
        app_name = data['name']
        
        # Check if deployment already exists
        try:
            existing_deployment = k8s_apps_v1.read_namespaced_deployment(
                name=app_name,
                namespace="auto-deploy"
            )
            logger.info(f"Deployment {app_name} already exists, updating...")
            
            # Update existing deployment
            deployment = create_deployment_object(app_name, image_name)
            k8s_apps_v1.patch_namespaced_deployment(
                name=app_name,
                namespace="auto-deploy",
                body=deployment
            )
        except client.exceptions.ApiException as e:
            if e.status == 404:
                logger.info(f"Creating new deployment for {app_name}")
                # Create new deployment
                deployment = create_deployment_object(app_name, image_name)
                k8s_apps_v1.create_namespaced_deployment(
                    body=deployment,
                    namespace="auto-deploy"
                )
                
                # Create service for new deployment
                service = create_service_object(app_name)
                k8s_core_v1.create_namespaced_service(
                    body=service,
                    namespace="auto-deploy"
                )
            else:
                raise

        return jsonify({
            "status": "success",
            "message": f"Deployed {app_name}",
            "deployment": {
                "name": app_name,
                "image": image_name,
                "namespace": "auto-deploy"
            }
        }), 200
    
    except ValueError as e:
        logger.error(f"Validation error: {str(e)}")
        return jsonify({
            "status": "error",
            "message": str(e),
            "error_type": "validation_error"
        }), 400
    
    except Exception as e:
        logger.error(f"Deployment failed: {str(e)}", exc_info=True)
        return jsonify({
            "status": "error",
            "message": "Internal deployment error",
            "error_type": "internal_error"
        }), 500

def create_deployment_object(app_name, image_name):
    """Create a Kubernetes deployment object"""
    return {
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": app_name,
            "namespace": "auto-deploy",
            "labels": {
                "app": app_name,
                "managed-by": "deploy-controller"
            }
        },
        "spec": {
            "replicas": 1,
            "selector": {
                "matchLabels": {
                    "app": app_name
                }
            },
            "template": {
                "metadata": {
                    "labels": {
                        "app": app_name
                    }
                },
                "spec": {
                    "containers": [{
                        "name": app_name,
                        "image": image_name,
                        "ports": [{
                            "containerPort": 80
                        }],
                        "resources": {
                            "limits": {
                                "cpu": "500m",
                                "memory": "512Mi"
                            },
                            "requests": {
                                "cpu": "100m",
                                "memory": "128Mi"
                            }
                        },
                        "livenessProbe": {
                            "httpGet": {
                                "path": "/",
                                "port": 80
                            },
                            "initialDelaySeconds": 30,
                            "periodSeconds": 10
                        }
                    }]
                }
            }
        }
    }

def create_service_object(app_name):
    """Create a Kubernetes service object"""
    return {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": f"{app_name}-service",
            "namespace": "auto-deploy",
            "labels": {
                "app": app_name,
                "managed-by": "deploy-controller"
            }
        },
        "spec": {
            "selector": {
                "app": app_name
            },
            "ports": [{
                "protocol": "TCP",
                "port": 80,
                "targetPort": 80
            }],
            "type": "ClusterIP"
        }
    }

if __name__ == "__main__":
    app.run(host='0.0.0.0', port=8080)
