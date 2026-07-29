//! Firecracker-containerd backend for game match infrastructure.
//!
//! See [`FirecrackerMachineProvider`] for the main entry point.

pub mod config;
pub mod network;
mod provider;
pub mod subnet_pool;

/// Generated firecracker-containerd control-plane **message types** (`fccontrol`).
///
/// Compiled by `build.rs` from the vendored protos in `proto/` (package-less, so
/// prost emits into the empty-package file). Only the prost messages are used —
/// the `Firecracker` control service is served over **ttrpc**, not gRPC, so the
/// client is hand-rolled in [`provider`] rather than generated here.
pub mod fccontrol {
    tonic::include_proto!("_");
}

pub use config::FirecrackerMachineProviderConfig;
pub use provider::{FirecrackerMachineProvider, FirecrackerMatchContext};
