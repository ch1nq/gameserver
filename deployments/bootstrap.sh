# Bootstrap the k3d cluster
k3d registry create achtung --port $REGISTRY_PORT
k3d cluster create --registry-use k3d-achtung:$REGISTRY_PORT -p "80:80@loadbalancer" -p "443:443@loadbalancer"

# Install cert-manager
helm repo add jetstack https://charts.jetstack.io
helm repo update
helm install cert-manager jetstack/cert-manager \
  --namespace cert-manager \
  --create-namespace \
  --version v1.13.3 \
  --set installCRDs=true

kubectl apply -f deployments/cluster-issuer.yaml

# Build and push the images to the local registry
docker build . -f apps/game-server/Dockerfile -t game-server:latest -t k3d-achtung:$REGISTRY_PORT/game-server:latest -t localhost:$REGISTRY_PORT/game-server:latest
docker push localhost:$REGISTRY_PORT/game-server:latest

docker build . -f apps/build-server/Dockerfile -t build-server:latest -t k3d-achtung:$REGISTRY_PORT/build-server:latest -t localhost:$REGISTRY_PORT/build-server:latest
docker push localhost:$REGISTRY_PORT/build-server:latest

docker build . -f apps/achtung-baseline/Dockerfile -t achtung-baseline:latest -t k3d-achtung:$REGISTRY_PORT/achtung-baseline:latest -t localhost:$REGISTRY_PORT/achtung-baseline:latest
docker push localhost:$REGISTRY_PORT/achtung-baseline:latest

docker build . -f apps/website/Dockerfile -t website:latest -t k3d-achtung:$REGISTRY_PORT/website:latest -t localhost:$REGISTRY_PORT/website:latest
docker push localhost:$REGISTRY_PORT/website:latest

# Deploy the services
kubectl apply \
    -f deployments/game-server.yaml \
    -f deployments/build-server.yaml \
    -f deployments/internal-registry.yaml \
    -f deployments/website.yaml
