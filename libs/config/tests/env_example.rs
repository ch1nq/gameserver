//! Acceptance: every `MACHINE_PROVIDER`, `DOCKER_*`, `MSB_*`, `GAME_*`,
//! `REAPER_*` (+ all other consumed vars) is documented in `.env.example`.
//! Prevents docs drifting from `env_names` (issue #27).

use achtung_config::ALL_ENV_VARS;

#[test]
fn env_example_documents_all_vars() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.example");
    let contents = std::fs::read_to_string(path).expect(".env.example must exist");
    let mut missing = Vec::new();
    for var in ALL_ENV_VARS {
        // Matches `VAR=` or `VAR =` or `# VAR=` (commented default counts as documented).
        let documented = contents.lines().any(|line| {
            let t = line.trim_start_matches('#').trim_start();
            t.starts_with(var)
                && t[var.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c == '=' || c == ' ' || c == ':')
        });
        if !documented {
            missing.push(*var);
        }
    }
    assert!(
        missing.is_empty(),
        ".env.example is missing vars (add them from env_names): {missing:?}"
    );
}
