use crate::agents::get_agents;
use crate::web::layouts::pages;
use axum::{response::IntoResponse, routing::get, Router};

pub fn router() -> Router<()> {
    Router::new().route("/", get(self::get::index))
}

mod get {
    use crate::users::AuthSession;

    use super::*;
    pub async fn index(auth_session: AuthSession) -> impl IntoResponse {
        pages::home(&auth_session, get_agents())
    }
}
