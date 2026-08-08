fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Client for the game host (GameHost.WatchGame upstream) plus server for the
    // browser-facing Spectator service the website relays through.
    tonic_build::configure().compile_protos(
        &[
            "../../protos/game_host.proto",
            "../../protos/spectator.proto",
        ],
        &["../../protos"],
    )?;
    Ok(())
}
