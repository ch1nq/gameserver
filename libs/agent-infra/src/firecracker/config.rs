//! Configuration for the firecracker-containerd machine provider.

use ipnet::Ipv4Net;

/// Configuration for the Firecracker machine provider.
#[derive(Debug, Clone)]
pub struct FirecrackerMachineProviderConfig {
    /// Path to the firecracker-containerd daemon socket.
    ///
    /// This must be the **dedicated firecracker-containerd** daemon, not vanilla
    /// containerd — only that daemon serves the `fccontrol` control API used to
    /// create microVMs. Defaults to `/run/firecracker-containerd/containerd.sock`.
    pub containerd_socket: String,

    /// containerd namespace used to isolate game resources.
    ///
    /// All containers, images, and snapshots for game matches are created under
    /// this namespace, keeping them separate from other containerd workloads.
    /// Defaults to `"achtung"`.
    pub containerd_namespace: String,

    /// containerd runtime to use for microVMs.
    ///
    /// For firecracker-containerd this is `"aws.firecracker"`.
    pub runtime: String,

    /// Path to the Linux kernel image (vmlinux) used by all microVMs.
    ///
    /// The kernel, its boot args, and the VM agent rootfs are normally configured
    /// as defaults in the firecracker-containerd runtime config
    /// (`/etc/containerd/firecracker-runtime.json`); this value is used to
    /// generate/verify that file during host setup. All microVMs share the same
    /// kernel; the OCI image supplies the *container* rootfs (mounted inside the VM).
    pub kernel_path: String,

    /// Number of vCPUs assigned to each microVM.
    pub vcpu_count: u32,

    /// Memory size (MiB) assigned to each microVM.
    pub mem_size_mib: u32,

    /// URL of the private container registry (e.g., `"http://registry:5001"`).
    ///
    /// Used when pulling private agent images.
    pub registry_url: String,

    /// Pool of IPv4 addresses from which per-match `/24` subnets are allocated.
    ///
    /// Should be a `/16` or larger private range unlikely to conflict with the
    /// host network. Defaults to `10.200.0.0/16`, giving up to 256 concurrent
    /// matches.
    pub subnet_pool: Ipv4Net,
}

impl Default for FirecrackerMachineProviderConfig {
    fn default() -> Self {
        Self {
            containerd_socket: "/run/firecracker-containerd/containerd.sock".into(),
            containerd_namespace: "achtung".into(),
            runtime: "aws.firecracker".into(),
            kernel_path: "/var/lib/firecracker-containerd/runtime/default-vmlinux.bin".into(),
            vcpu_count: 1,
            mem_size_mib: 512,
            registry_url: "http://localhost:5001".into(),
            subnet_pool: "10.200.0.0/16".parse().expect("valid default subnet"),
        }
    }
}
