//! Local Docker [`MachineProvider`].
//!
//! Runs the game host and agent containers on the local Docker daemon, on a
//! shared user-defined network so the coordinator (running in the website
//! container) can reach them by container name via Docker's embedded DNS.
//!
//! Addressing: each container is named `achtung-{match}-slot-{n}` and that name
//! is used as its `private_ip`, so the coordinator dials `http://{name}:50051`
//! (game host) / `{name}:50052` (agents), resolved on the shared network.
//!
//! Images: the game host is a public/local image (pulled only if absent, so a
//! locally-built tag works); agent images are pulled from the private registry
//! using the coordinator's scoped deploy token as a bearer credential.
//!
//! This mode provides **no** isolation between agents beyond stock runc — every
//! container shares one L2 segment and has outbound internet. It is the local
//! development loop, not the production sandbox.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bollard::Docker;
use bollard::auth::DockerCredentials;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use futures_util::StreamExt;

use crate::{
    ContainerImage, MachineError, MachineHandle, MachineProvider, OrphanedResource, SpawnConfig,
};

/// Configuration for the local Docker machine provider.
#[derive(Debug, Clone)]
pub struct DockerMachineProviderConfig {
    /// Docker network to attach match containers to. Must be the same network
    /// the website/coordinator container is on so it can resolve them by name.
    pub network: String,
    /// Registry host used to build private image pull refs, reachable by the
    /// Docker daemon (e.g. `localhost:5001`, which the daemon treats as
    /// insecure automatically).
    pub registry_pull_host: String,
}

/// Per-match context. Docker needs no shared per-match resources (containers
/// share one network), so this just carries the match id used to name them.
pub struct DockerMatchContext {
    match_id: String,
}

/// Local Docker implementation of [`MachineProvider`].
pub struct DockerMachineProvider {
    docker: Docker,
    config: DockerMachineProviderConfig,
}

impl DockerMachineProvider {
    /// Connect to the local Docker daemon over its default unix socket.
    pub fn new(config: DockerMachineProviderConfig) -> Result<Self, MachineError> {
        let docker = Docker::connect_with_unix_defaults()
            .map_err(|e| MachineError::MatchInit(format!("connect to docker daemon: {e}")))?;
        Ok(Self { docker, config })
    }

    /// Split an image ref into `(name, tag)` for the pull API. Handles the
    /// registry-port colon (e.g. `localhost:5001/x/y:tag`) by only treating the
    /// last `:` as a tag separator when it has no `/` after it.
    fn split_ref(image: &str) -> (String, String) {
        match image.rsplit_once(':') {
            Some((name, tag)) if !tag.contains('/') => (name.to_string(), tag.to_string()),
            _ => (image.to_string(), "latest".to_string()),
        }
    }

    async fn pull(
        &self,
        image: &str,
        credentials: Option<DockerCredentials>,
    ) -> Result<(), MachineError> {
        let (from_image, tag) = Self::split_ref(image);
        let options = CreateImageOptions {
            from_image,
            tag,
            ..Default::default()
        };
        let mut stream = self.docker.create_image(Some(options), None, credentials);
        while let Some(item) = stream.next().await {
            item.map_err(|e| MachineError::ImageCopy(format!("pull {image}: {e}")))?;
        }
        Ok(())
    }

    /// Resolve `config.container_image` to a locally-available image ref,
    /// pulling as needed. Returns the ref to run.
    async fn ensure_image(&self, image: &ContainerImage) -> Result<String, MachineError> {
        match image {
            // Public/local: pull only if absent, so a locally-built tag works.
            ContainerImage::Public(url) => {
                let reference = url.as_ref().to_string();
                if self.docker.inspect_image(&reference).await.is_err() {
                    self.pull(&reference, None).await?;
                }
                Ok(reference)
            }
            // Private agent: pull from the registry with the deploy token as a
            // bearer credential (X-Registry-Auth registrytoken).
            ContainerImage::Private {
                image_url,
                registry_token,
            } => {
                let reference =
                    format!("{}/{}", self.config.registry_pull_host, image_url.as_ref());
                let credentials = DockerCredentials {
                    registrytoken: Some(registry_token.as_ref().to_string()),
                    ..Default::default()
                };
                self.pull(&reference, Some(credentials)).await?;
                Ok(reference)
            }
        }
    }

    /// Whether a bollard error is a 404 (resource already gone).
    fn is_not_found(err: &bollard::errors::Error) -> bool {
        matches!(
            err,
            bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                ..
            }
        )
    }
}

fn container_name(match_id: &str, slot: u8) -> String {
    format!("achtung-{match_id}-slot-{slot}")
}

#[async_trait::async_trait]
impl MachineProvider for DockerMachineProvider {
    type MatchContext = DockerMatchContext;

    async fn init_match(&self, match_id: &str) -> Result<DockerMatchContext, MachineError> {
        Ok(DockerMatchContext {
            match_id: match_id.to_string(),
        })
    }

    async fn spawn(
        &self,
        ctx: &DockerMatchContext,
        config: SpawnConfig,
    ) -> Result<MachineHandle, MachineError> {
        let name = container_name(&ctx.match_id, config.slot);
        let image = self.ensure_image(&config.container_image).await?;

        let env: Vec<String> = config.env.iter().map(|(k, v)| format!("{k}={v}")).collect();

        let container_config = Config {
            image: Some(image.clone()),
            env: Some(env),
            host_config: Some(HostConfig {
                network_mode: Some(self.config.network.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };

        self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name.clone(),
                    platform: None,
                }),
                container_config,
            )
            .await
            .map_err(|e| MachineError::MachineCreation(format!("create {name}: {e}")))?;

        self.docker
            .start_container::<String>(&name, None)
            .await
            .map_err(|e| MachineError::MachineCreation(format!("start {name}: {e}")))?;

        tracing::info!(
            match_id = ctx.match_id,
            container = name,
            image,
            slot = config.slot,
            "Spawned Docker container"
        );

        Ok(MachineHandle {
            app_name: ctx.match_id.clone(),
            machine_id: name.clone(),
            // Container name doubles as the DNS-resolvable address on the network.
            private_ip: name,
        })
    }

    async fn destroy(
        &self,
        _ctx: &DockerMatchContext,
        handle: &MachineHandle,
    ) -> Result<(), MachineError> {
        match self
            .docker
            .remove_container(
                &handle.machine_id,
                Some(RemoveContainerOptions {
                    force: true,
                    v: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e) if Self::is_not_found(&e) => Ok(()),
            Err(e) => Err(MachineError::Destruction(format!(
                "remove {}: {e}",
                handle.machine_id
            ))),
        }
    }

    async fn cleanup_match(&self, _ctx: DockerMatchContext) -> Result<(), MachineError> {
        // Containers share the pre-existing network; nothing per-match to release.
        Ok(())
    }

    async fn list_orphaned(
        &self,
        prefix: &str,
        max_age: Duration,
    ) -> Result<Vec<OrphanedResource>, MachineError> {
        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await
            .map_err(|e| MachineError::Destruction(format!("list containers: {e}")))?;

        let now = SystemTime::now();
        let mut orphaned = Vec::new();

        for c in containers {
            let name = c
                .names
                .unwrap_or_default()
                .into_iter()
                .next()
                .unwrap_or_default();
            let name = name.trim_start_matches('/').to_string();
            if !name.starts_with(prefix) {
                continue;
            }

            let created_at = c
                .created
                .filter(|&s| s >= 0)
                .map(|s| UNIX_EPOCH + Duration::from_secs(s as u64))
                .unwrap_or(now);

            if now.duration_since(created_at).unwrap_or(Duration::ZERO) >= max_age {
                orphaned.push(OrphanedResource {
                    id: c.id.unwrap_or_else(|| name.clone()),
                    name,
                    created_at,
                });
            }
        }

        tracing::info!(
            count = orphaned.len(),
            prefix,
            "Docker orphan scan complete"
        );
        Ok(orphaned)
    }

    async fn destroy_orphaned(&self, resource: &OrphanedResource) -> Result<(), MachineError> {
        match self
            .docker
            .remove_container(
                &resource.id,
                Some(RemoveContainerOptions {
                    force: true,
                    v: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e) if Self::is_not_found(&e) => Ok(()),
            Err(e) => Err(MachineError::Destruction(format!(
                "remove orphan {}: {e}",
                resource.id
            ))),
        }
    }
}
