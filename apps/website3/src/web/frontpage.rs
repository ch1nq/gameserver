use askama::Template;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

#[derive(Template)]
#[template(path = "index.html")]
struct FrontpageTemplate;

pub fn router() -> Router<()> {
    Router::new().route("/", get(self::get::index))
}

mod get {
    use super::*;

    pub async fn index() -> impl IntoResponse {
        Html(FrontpageTemplate.render().unwrap()).into_response()
    }
}
