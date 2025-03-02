use askama::Template;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

pub fn router() -> Router<()> {
    Router::new().route("/", get(self::get::index))
}

use crate::users::User;

#[derive(Template)]
#[template(path = "pages/index.html")]
struct FrontpageTemplate {
    user: Option<User>,
}

mod get {
    use super::*;
    pub async fn index() -> impl IntoResponse {
        Html(FrontpageTemplate { user: None }.render().unwrap()).into_response()
    }
}
