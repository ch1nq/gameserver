default: build-and-push-images

registry_port := "43365"
build_version := `git rev-parse --short HEAD`

build-and-push-images:
    # Build and push the images to the local registry

    for app in game-server build-server achtung-baseline website; do \
        echo "Building and pushing $app"; \
        docker build . -f apps/$app/Dockerfile \
            -t $app:{{build_version}} \
            -t $app:latest \
            -t k3d-achtung:{{registry_port}}/$app:{{build_version}} \
            -t k3d-achtung:{{registry_port}}/$app:latest \
            -t localhost:{{registry_port}}/$app:{{build_version}} \
            -t localhost:{{registry_port}}/$app:latest \
            -t localhost:{{registry_port}}/$app:latest; \
        docker push localhost:{{registry_port}}/$app; \
    done

deploy-all:
    kubectl apply \
        -f deployments/game-server.yaml \
        -f deployments/build-server.yaml \
        -f deployments/internal-registry.yaml \
        -f deployments/website.yaml

deploy-website:
    kubectl apply -f deployments/website.yaml
    kubectl rollout restart deployment/website

bootstrap-cluster:
    # Bootstrap the k3d cluster
    k3d registry create achtung --port {{registry_port}}
    k3d cluster create --registry-use k3d-achtung:{{registry_port}} -p "80:80@loadbalancer" -p "443:443@loadbalancer"

    # Install cert-manager
    helm repo add jetstack https://charts.jetstack.io
    helm repo update
    helm install cert-manager jetstack/cert-manager \
        --namespace cert-manager \
        --create-namespace \
        --version v1.13.3 \
        --set installCRDs=true

    # Install the cluster issuer before deploying the apps
    kubectl apply -f deployments/cluster-issuer.yaml

    build-and-push-images
    deploy-all
