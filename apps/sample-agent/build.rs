fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(false)
        .compile_protos(&["../../protos/achtung_agent.proto"], &["../../protos"])?;
    Ok(())
}
