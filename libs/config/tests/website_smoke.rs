use achtung_config::{WebsiteConfig, env_names};
use std::collections::HashMap;

fn base_map() -> HashMap<String, String> {
    HashMap::from([
        (env_names::GITHUB_CLIENT_ID.to_string(), "id".to_string()),
        (
            env_names::GITHUB_CLIENT_SECRET.to_string(),
            "secret".to_string(),
        ),
        (
            env_names::DATABASE_URL.to_string(),
            "postgresql://localhost/db".to_string(),
        ),
        (
            env_names::REGISTRY_PRIVATE_KEY.to_string(),
            "pem".to_string(),
        ),
    ])
}

#[test]
fn minimal_without_coordinator_disables_it() {
    let cfg = WebsiteConfig::from_map(base_map()).unwrap();
    assert!(cfg.coordinator.is_none());
    assert_eq!(cfg.server.port, 3000);
    assert_eq!(cfg.registry_service, "registry:5001");
}

#[test]
fn missing_required_gives_single_missing_error() {
    let mut m = base_map();
    m.remove(env_names::DATABASE_URL);
    let err = WebsiteConfig::from_map(m).unwrap_err();
    assert!(err.to_string().contains(env_names::DATABASE_URL), "{err}");
}

#[test]
fn unknown_provider_fails_fast() {
    let mut m = base_map();
    m.insert(
        env_names::ENABLE_COORDINATOR.to_string(),
        "true".to_string(),
    );
    m.insert(env_names::MACHINE_PROVIDER.to_string(), "bogus".to_string());
    let err = WebsiteConfig::from_map(m).unwrap_err();
    assert!(
        err.to_string().contains(env_names::MACHINE_PROVIDER),
        "{err}"
    );
}

#[test]
fn docker_without_network_fails() {
    let mut m = base_map();
    m.insert(env_names::ENABLE_COORDINATOR.to_string(), "1".to_string());
    m.insert(
        env_names::MACHINE_PROVIDER.to_string(),
        "docker".to_string(),
    );
    let err = WebsiteConfig::from_map(m).unwrap_err();
    assert!(err.to_string().contains(env_names::DOCKER_NETWORK), "{err}");
}

#[test]
fn enable_coordinator_bare_presence_means_true() {
    let mut m = base_map();
    m.insert(env_names::ENABLE_COORDINATOR.to_string(), String::new());
    m.insert(env_names::DOCKER_NETWORK.to_string(), "net".to_string());
    m.insert(
        env_names::MACHINE_PROVIDER.to_string(),
        "docker".to_string(),
    );
    let cfg = WebsiteConfig::from_map(m).unwrap();
    assert!(cfg.coordinator.is_some());
}

#[test]
fn enable_coordinator_false_disables() {
    let mut m = base_map();
    m.insert(
        env_names::ENABLE_COORDINATOR.to_string(),
        "false".to_string(),
    );
    let cfg = WebsiteConfig::from_map(m).unwrap();
    assert!(cfg.coordinator.is_none());
}

#[test]
fn garbage_number_fails_not_silent_default() {
    let mut m = base_map();
    m.insert(
        env_names::ENABLE_COORDINATOR.to_string(),
        "true".to_string(),
    );
    m.insert(env_names::AGENTS_PER_GAME.to_string(), "bogus".to_string());
    let err = WebsiteConfig::from_map(m).unwrap_err();
    assert!(
        err.to_string().contains(env_names::AGENTS_PER_GAME),
        "{err}"
    );
}
