use achtung_config::{CliConfig, GameHostConfig, env_names};
use std::collections::HashMap;

#[test]
fn game_host_defaults_are_unified_1000_squared() {
    let cfg = GameHostConfig::from_map(HashMap::new()).unwrap();
    assert_eq!(cfg.port, 50051);
    assert_eq!(cfg.arena_width, 1000);
    assert_eq!(cfg.arena_height, 1000);
}

#[test]
fn game_host_rejects_zero_arena() {
    let m = HashMap::from([(env_names::ARENA_WIDTH.to_string(), "0".to_string())]);
    let err = GameHostConfig::from_map(m).unwrap_err();
    assert!(err.to_string().contains(env_names::ARENA_WIDTH), "{err}");
}

#[test]
fn game_host_rejects_garbage_port() {
    let m = HashMap::from([(env_names::PORT.to_string(), "bogus".to_string())]);
    let err = GameHostConfig::from_map(m).unwrap_err();
    assert!(err.to_string().contains(env_names::PORT), "{err}");
}

#[test]
fn cli_missing_api_url_is_single_error() {
    let err = CliConfig::from_map(HashMap::new()).unwrap_err();
    assert!(
        err.to_string().contains(env_names::ACHTUNG_API_URL),
        "{err}"
    );
}

#[test]
fn cli_env_wins_and_registry_defaults() {
    let m = HashMap::from([
        (
            env_names::ACHTUNG_API_URL.to_string(),
            "http://x".to_string(),
        ),
        (env_names::ACHTUNG_USER_ID.to_string(), "7".to_string()),
        (env_names::ACHTUNG_API_TOKEN.to_string(), "tok".to_string()),
    ]);
    let cfg = CliConfig::from_map(m).unwrap();
    assert_eq!(cfg.api_url, "http://x");
    assert_eq!(cfg.user_id, 7);
    assert_eq!(cfg.registry_host, "localhost:5001");
}

#[test]
fn cli_rejects_non_integer_user_id() {
    let m = HashMap::from([
        (
            env_names::ACHTUNG_API_URL.to_string(),
            "http://x".to_string(),
        ),
        (env_names::ACHTUNG_USER_ID.to_string(), "bogus".to_string()),
        (env_names::ACHTUNG_API_TOKEN.to_string(), "tok".to_string()),
    ]);
    let err = CliConfig::from_map(m).unwrap_err();
    assert!(
        err.to_string().contains(env_names::ACHTUNG_USER_ID),
        "{err}"
    );
}

#[test]
fn cli_file_layer_fills_gaps_and_env_wins() {
    use achtung_config::CliFileParsed;
    let file = CliFileParsed {
        api_url: Some("http://file".to_string()),
        user_id: Some(3),
        api_token: Some("file-token".to_string()),
        registry_host: Some("file-host:5001".to_string()),
    };
    // File alone suffices; registry_host falls back to its default.
    let bare = CliFileParsed {
        registry_host: None,
        ..file.clone()
    };
    let cfg = CliConfig::from_map_with_file(HashMap::new(), Some(bare)).unwrap();
    assert_eq!(cfg.api_url, "http://file");
    assert_eq!(cfg.user_id, 3);
    assert_eq!(cfg.registry_host, "localhost:5001");

    // Env overrides the file on every field it sets.
    let m = HashMap::from([
        (
            env_names::ACHTUNG_API_URL.to_string(),
            "http://env".to_string(),
        ),
        (env_names::ACHTUNG_USER_ID.to_string(), "9".to_string()),
    ]);
    let cfg = CliConfig::from_map_with_file(m, Some(file)).unwrap();
    assert_eq!(cfg.api_url, "http://env");
    assert_eq!(cfg.user_id, 9);
    assert_eq!(cfg.api_token, "file-token");
    assert_eq!(cfg.registry_host, "file-host:5001");
}
