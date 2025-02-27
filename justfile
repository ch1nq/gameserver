default: build-all

registry_host := "localhost"
registry_port := "43365"
build_version := `git rev-parse --short HEAD`

build app registry_host=registry_host registry_port=registry_port:
    # Build and push the images to the local registry
    @echo "Building and pushing {{app}}:{{build_version}}"
    docker build . -f apps/{{app}}/Dockerfile \
        -t {{app}}:{{build_version}} \
        -t {{app}}:latest \
        -t k3d-achtung:{{registry_port}}/{{app}}:{{build_version}} \
        -t k3d-achtung:{{registry_port}}/{{app}}:latest \
        -t {{registry_host}}:{{registry_port}}/{{app}}:{{build_version}} \
        -t {{registry_host}}:{{registry_port}}/{{app}}:latest
    docker push {{registry_host}}:{{registry_port}}/{{app}}

build-all:
    for app in $(ls apps | tr '\n' ' '); do \
        just build-and-push $app; \
    done

deploy deployment:
    kubectl apply -f deployments/{{deployment}}.yaml
    kubectl rollout restart deployment/{{deployment}}

deploy-all:
    kubectl apply -f deployments/game-server.yaml
    kubectl apply -f deployments/internal-registry.yaml
    kubectl apply -f deployments/build-server.yaml
    kubectl apply -f deployments/website.yaml

undeploy-all:
    kubectl delete -f deployments/game-server.yaml 
    kubectl delete -f deployments/internal-registry.yaml 
    kubectl delete -f deployments/build-server.yaml 
    kubectl delete -f deployments/website.yaml

watch:
    watch kubectl get all

deploy-baseline-agents:
    build achtung-baseline
    curl -X POST localhost:30109/deploy \
        -H 'Content-Type: application/json' \
        -d '{ "name":"baseline", "image": "k3d-achtung:43365/achtung-baseline:{{build_version}}" }'

configure-gh-oauth gh_oauth_id gh_oauth_secret:
    kubectl create secret generic github-oauth-app \
        --from-literal=CLIENT_ID={{gh_oauth_id}} \
        --from-literal=CLIENT_SECRET={{gh_oauth_secret}}
    kubectl apply -f deployments/website.yaml

bootstrap-cluster:
    # Bootstrap the k3d cluster
    k3d registry create achtung --port {{registry_port}}
    k3d cluster create --registry-use k3d-achtung:{{registry_port}} \
        -p "80:80@loadbalancer" -p "443:443@loadbalancer" -p "30109:30109@loadbalancer"

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
    

destroy-cluster:
    k3d cluster delete achtung
    k3d registry delete achtung
