fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Vendored from firecracker-containerd (proto/{types,firecracker}.proto and
    // proto/service/fccontrol/fccontrol.proto). We only need the prost *message*
    // types (CreateVMRequest, StopVMRequest, …): the control service is served
    // over **ttrpc**, not gRPC (see `firecracker::control`), so we disable both
    // client and server code generation and hand-roll the ttrpc client.
    tonic_build::configure()
        .build_client(false)
        .build_server(false)
        .compile_protos(&["proto/fccontrol.proto"], &["proto"])?;
    Ok(())
}
