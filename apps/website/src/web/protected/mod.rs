mod agents;
mod settings;

use crate::web::app::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/agents", agents::router())
        .nest("/settings", settings::router())
}
