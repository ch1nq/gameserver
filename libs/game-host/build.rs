fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Server for the generic GameHost service (package `gamehost`) that the
    // coordinator drives, plus a client for Achtung's typed Agent service
    // (`achtung.agent`) that the host dials each tick.
    tonic_build::configure().compile_protos(
        &[
            "../../protos/game_host.proto",
            "../../protos/achtung_agent.proto",
        ],
        &["../../protos"],
    )?;
    Ok(())
}
