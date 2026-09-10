fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Server for the generic GameHost service (package `gamehost`) that the
    // coordinator drives, plus a client for Achtung's typed Agent service
    // (`achtung.agent`) that the host dials each tick.
    //
    // `achtung.spectator` types get `serde::Serialize` so the host can render
    // browser JSON once per frame (see `GameAdapter::tick_spectator`). The
    // website relay treats that JSON as opaque.
    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Serialize)]")
        .compile_protos(
            &[
                "../../protos/game_host.proto",
                "../../protos/spectator_frame.proto",
                "../../protos/achtung_agent.proto",
                "../../protos/achtung_spectator.proto",
            ],
            &["../../protos"],
        )?;
    Ok(())
}
