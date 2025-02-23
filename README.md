# Arcadio

Arcadio is a competitive platform where developers battle by creating autonomous agents that compete in "Achtung! Die Kurve" (also known as Curve Fever or Zatacka). Build your AI agent using our SDK, test it against others, and climb the global leaderboard

Try it now at: https://achtung.daske.dk

<img width="1023" alt="Arcadio preview" src="https://github.com/user-attachments/assets/0a235aaf-2f81-44e8-8957-32027ccd6d88" />

## Development

### Monorepo structure
The Arcadio monorepo is organized into three main directories:
- `libs`: Projects in the libs folder define reusable components, utilities, and shared business logic. These packages can be dependencies for other libs or apps
- `apps`: Each app is a deployable unit with its own Dockerfile. Apps can depend on packages from the libs directory but cannot depend on other apps.
- `deployments`: Contains Kubernetes manifests for deploying to a cluster.


## Deployment
Prerequisits:
- `k3d`, `just`, `docker`
- A github oauth app

Then you can run
```shell
just bootstrap-cluster
just configure-gh-oauth <GITHUB_OAUTH_APP_ID> <GITHUB_OAUTH_SECRET>
just build-all deploy-all
```

Once you are done you can destroy the cluster again
```shell
just destroy-cluster
```
