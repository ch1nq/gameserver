//! Bootstrap a local-dev user with the two tokens the local match workflow
//! needs: a *registry token* (for `docker login`/push of agent images) and an
//! *API token* (for the `achtung` CLI). Agents themselves are managed with the
//! CLI (`achtung agent create/activate ...`), not seeded here.
//!
//!   DATABASE_URL=postgres://arcadio:arcadio@localhost:5432/arcadio \
//!   REGISTRY_PRIVATE_KEY="$(docker exec arcadio-website printenv REGISTRY_PRIVATE_KEY)" \
//!   cargo run -p achtung-core --example seed_local

use achtung_core::api_tokens::ApiTokenManager;
use achtung_core::registry::RegistryTokenManager;
use registry_auth::{RegistryAuthConfig, TokenName};

#[tokio::main]
async fn main() {
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let db = achtung_core::db::connect_and_migrate(&db_url)
        .await
        .expect("connect to database");

    // 1. Ensure a local-dev user exists (users are normally created via OAuth).
    let user_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO users (username, access_token) VALUES ($1, $2)
        ON CONFLICT (username) DO UPDATE SET access_token = EXCLUDED.access_token
        RETURNING id
        "#,
    )
    .bind("local-dev")
    .bind("local-dev")
    .fetch_one(&db)
    .await
    .expect("upsert local-dev user");

    // 2. A registry token for `docker login` / push to the local registry.
    let pem = std::env::var("REGISTRY_PRIVATE_KEY").expect("REGISTRY_PRIVATE_KEY required");
    let service = std::env::var("REGISTRY_SERVICE").unwrap_or_else(|_| "registry:5001".to_string());
    let auth_config = RegistryAuthConfig::new(pem, service).expect("registry auth config");
    let registry_tokens = RegistryTokenManager::new(db.clone(), auth_config);
    let registry_token = registry_tokens
        .create_token(
            &user_id,
            &TokenName::new("local-dev".to_string()).expect("name"),
        )
        .await
        .expect("create registry token");

    // 3. An API token for the `achtung` CLI (agent management).
    let api_tokens = ApiTokenManager::new(db.clone());
    let api_token = api_tokens
        .create_token(
            &user_id,
            &TokenName::new("local-dev-cli".to_string()).expect("name"),
        )
        .await
        .expect("create api token");

    println!("---- bootstrapped ----");
    println!("USER_ID={user_id}");
    println!(
        "REGISTRY_TOKEN={}   # docker login localhost:5001 -u user-{user_id}",
        registry_token.as_ref()
    );
    println!(
        "API_TOKEN={}   # ACHTUNG_API_TOKEN for the achtung CLI",
        api_token.as_ref()
    );
}
