use crate::agents::get_agents;
use axum::{
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};

use crate::users::AuthSession;

pub fn router() -> Router<()> {
    Router::new()
        .route("/agents", get(self::get::agents))
        .route("/agents/new", post(self::post::new_agent))
        .route("/settings", get(self::get::settings))
}

mod get {
    use crate::web::layouts::pages;

    use super::*;

    pub async fn agents(auth_session: AuthSession) -> impl IntoResponse {
        pages::agents(&auth_session, get_agents())
    }

    pub async fn settings(auth_session: AuthSession) -> impl IntoResponse {
        pages::settings(&auth_session)
    }
}

mod post {

    use super::*;

    pub async fn new_agent() -> impl IntoResponse {
        Redirect::to("/agents").into_response()
    }
}
