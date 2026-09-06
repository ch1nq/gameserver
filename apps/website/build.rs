fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Message-only proto for the achtung spectator payload. The SSE handler
    // decodes SpectatorFrame.payload into these and re-emits them as JSON, so
    // the browser needs no protobuf runtime. `serde::Serialize` is derived so
    // the decoded types can be handed straight to `serde_json`.
    tonic_build::configure()
        .build_client(false)
        .build_server(false)
        .type_attribute(".", "#[derive(serde::Serialize)]")
        .compile_protos(&["../../protos/achtung_spectator.proto"], &["../../protos"])?;
    Ok(())
}
