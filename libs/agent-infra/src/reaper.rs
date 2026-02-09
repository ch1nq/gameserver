//! Reaper for cleaning up orphaned match infrastructure.
//!
//! This module provides a platform-agnostic reaper that periodically scans for
//! and destroys orphaned apps/machines that were not properly cleaned up after
//! game matches ended.

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::{MachineProvider, OrphanedResource};

/// Configuration for the infrastructure reaper
#[derive(Debug, Clone)]
pub struct ReaperConfig {
    /// How often to run the reaper scan
    pub interval: Duration,
    /// Apps older than this threshold are considered dead
    pub max_age: Duration,
    /// Prefix pattern to match app names (e.g., "achtung-match-")
    pub prefix: String,
}

/// Infrastructure reaper that cleans up orphaned match apps
///
/// The reaper runs as a background task, periodically scanning for apps that
/// match a naming pattern and are candidates for cleanup. This is used to
/// clean up infrastructure that failed to be destroyed properly due to errors,
/// crashes, or other issues.
pub struct Reaper<P: MachineProvider> {
    provider: P,
    config: ReaperConfig,
}

impl<P: MachineProvider> Reaper<P> {
    /// Create a new reaper with the given provider and configuration
    pub fn new(provider: P, config: ReaperConfig) -> Self {
        Self { provider, config }
    }

    /// Spawn the reaper as a background task
    ///
    /// The reaper will run indefinitely, performing cleanup scans at the
    /// configured interval.
    pub fn spawn(self) -> JoinHandle<()>
    where
        P: Send + Sync + 'static,
    {
        tokio::spawn(async move {
            tracing::info!(
                "Reaper started: interval={:?}, max_age={:?}, prefix={}",
                self.config.interval,
                self.config.max_age,
                self.config.prefix
            );

            loop {
                self.reap_once().await;
                tokio::time::sleep(self.config.interval).await;
            }
        })
    }

    /// Perform a single reaping scan
    async fn reap_once(&self) {
        tracing::debug!("Starting reap cycle");

        match self
            .provider
            .list_orphaned(&self.config.prefix, self.config.max_age)
            .await
        {
            Ok(orphans) => {
                if orphans.is_empty() {
                    tracing::debug!("No orphaned apps found");
                    return;
                }

                tracing::info!("Found {} orphaned apps to reap", orphans.len());

                let mut reaped_count = 0;
                let mut failed_count = 0;

                for infra in orphans {
                    match self.destroy_orphan(&infra).await {
                        Ok(()) => {
                            reaped_count += 1;
                        }
                        Err(()) => {
                            failed_count += 1;
                        }
                    }
                }

                tracing::info!(
                    "Reap cycle complete: reaped={}, failed={}",
                    reaped_count,
                    failed_count
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to list orphaned apps");
            }
        }
    }

    /// Destroy a single orphaned infrastructure item
    ///
    /// Returns Ok(()) on success, Err(()) on failure. The error is logged
    /// internally - this is designed to not propagate errors so one failure
    /// doesn't prevent cleanup of other orphans.
    async fn destroy_orphan(&self, resource: &OrphanedResource) -> Result<(), ()> {
        match self.provider.destroy_orphaned(resource).await {
            Ok(()) => {
                tracing::info!(
                    app = %resource.name,
                    id = %resource.id,
                    "Successfully reaped orphaned app"
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    app = %resource.name,
                    id = %resource.id,
                    error = %e,
                    "Failed to reap orphaned app"
                );
                Err(())
            }
        }
    }
}
