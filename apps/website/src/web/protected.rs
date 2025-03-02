use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};

use crate::users::{AuthSession, User};

#[derive(Template)]
#[template(path = "pages/protected.html")]
struct ProtectedTemplate {
    user: Option<User>,
}

#[derive(Debug, Clone)]
struct Agent {
    name: String,
    status: String,
}

#[derive(Template)]
#[template(path = "pages/agents.html")]
struct ManageAgentsTemplate {
    user: Option<User>,
    agents: Vec<Agent>,
}

#[derive(Template)]
#[template(path = "pages/new_agent.html")]
struct NewAgentTemplate {
    user: Option<User>,
}

pub fn router() -> Router<()> {
    Router::new()
        .route("/protected", get(self::get::protected))
        .route("/agents", get(self::get::agents))
        .route("/agents/new", get(self::get::new_agent))
        .route("/agents/new", post(self::post::new_agent))
}

mod get {

    use super::*;

    pub async fn protected(auth_session: AuthSession) -> impl IntoResponse {
        match auth_session.user {
            Some(user) => {
                Html(ProtectedTemplate { user: Some(user) }.render().unwrap()).into_response()
            }

            None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }

    pub async fn agents(auth_session: AuthSession) -> impl IntoResponse {
        let agents = vec![
            Agent {
                name: "Alice".to_string(),
                status: "Active".to_string(),
            },
            Agent {
                name: "Bob".to_string(),
                status: "Inactive".to_string(),
            },
        ];

        Html(
            ManageAgentsTemplate {
                user: auth_session.user,
                agents,
            }
            .render()
            .unwrap(),
        )
        .into_response()
    }

    pub async fn new_agent(auth_session: AuthSession) -> impl IntoResponse {
        Html(
            NewAgentTemplate {
                user: auth_session.user,
            }
            .render()
            .unwrap(),
        )
        .into_response()
    }
}

mod post {

    use super::*;

    pub async fn new_agent() -> impl IntoResponse {
        Redirect::to("/agents").into_response()
    }
}
