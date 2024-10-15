# Deploy on GCP
First build the docker images and push them to the GCP container registry.
```
gcloud builds submit --tag europe-west1-docker.pkg.dev/tactiqal/hello-repo/gameserver ./server
gcloud builds submit --tag europe-west1-docker.pkg.dev/tactiqal/hello-repo/gameclient ./client
```

Once the builds have completed, deploy the images to the GKE cluster.
```
kubectl apply -f server/deployment.yaml -f server/service.yaml -f client/deployment.yaml
```
