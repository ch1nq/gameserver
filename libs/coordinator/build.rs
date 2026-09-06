fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Client for the game host (GameHost.WatchGame upstream). The browser-facing
    // relay is now an SSE handler in the website; the shared SpectatorFrame is
    // still needed to decode the WatchGame stream.
    tonic_build::configure().compile_protos(
        &[
            "../../protos/game_host.proto",
            "../../protos/spectator_frame.proto",
        ],
        &["../../protos"],
    )?;
    Ok(())
}
