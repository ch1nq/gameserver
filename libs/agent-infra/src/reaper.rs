//! Reaper for cleaning up orphaned match infrastructure.
//!
//! Periodically scans for and destroys orphaned resources (apps/machines/containers)
//! that were not properly cleaned up after game matches ended.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::{MachineProvider, OrphanedResource};

/// Configuration for the infrastructure reaper
#[derive(Debug, Clone)]
pub struct ReaperConfig {
    /// How often to run the reaper scan
    pub interval: Duration,
    /// Resources older than this threshold are considered orphaned
    pub max_age: Duration,
    /// Prefix pattern to match resource names (e.g., "achtung-match-")
    pub prefix: String,
}

/// Infrastructure reaper that cleans up orphaned match resources.
///
/// Runs as a background task, periodically scanning for resources that match
/// a naming pattern and are older than `max_age`. Shares the provider via
/// `Arc<P>` with the coordinator.
pub struct Reaper<P: MachineProvider> {
    provider: Arc<P>,
    config: ReaperConfig,
}

impl<P: MachineProvider> Reaper<P> {
    pub fn new(provider: Arc<P>, config: ReaperConfig) -> Self {
        Self { provider, config }
    }

    /// Spawn the reaper as a background task
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!(
                interval = ?self.config.interval,
                max_age = ?self.config.max_age,
                prefix = self.config.prefix,
                "Reaper started"
            );
            loop {
                self.reap_once().await;
                tokio::time::sleep(self.config.interval).await;
            }
        })
    }

    async fn reap_once(&self) {
        tracing::debug!("Starting reap cycle");

        match self
            .provider
            .list_orphaned(&self.config.prefix, self.config.max_age)
            .await
        {
            Ok(orphans) if orphans.is_empty() => {
                tracing::debug!("No orphaned resources found");
            }
            Ok(orphans) => {
                tracing::info!(count = orphans.len(), "Reaping orphaned resources");
                let mut reaped = 0u32;
                let mut failed = 0u32;
                for resource in orphans {
                    match self.destroy_orphan(&resource).await {
                        Ok(()) => reaped += 1,
                        Err(()) => failed += 1,
                    }
                }
                tracing::info!(reaped, failed, "Reap cycle complete");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to list orphaned resources");
            }
        }
    }

    async fn destroy_orphan(&self, resource: &OrphanedResource) -> Result<(), ()> {
        match self.provider.destroy_orphaned(resource).await {
            Ok(()) => {
                tracing::info!(id = %resource.id, name = %resource.name, "Reaped orphaned resource");
                Ok(())
            }
            Err(e) => {
                tracing::warn!(id = %resource.id, name = %resource.name, error = %e, "Failed to reap orphaned resource");
                Err(())
            }
        }
    }
}
