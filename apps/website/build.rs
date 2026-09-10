fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No codegen: the spectator SSE relay is game-agnostic and forwards the
    // host-rendered `SpectatorFrame.json` opaquely. Game-specific spectator
    // protos live in the game host, which owns the schema and the JSON.
    Ok(())
}
