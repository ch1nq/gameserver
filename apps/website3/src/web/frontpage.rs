use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

#[derive(Template)]
#[template(path = "index.html")]
struct FrontpageTemplate {
    // Add any fields you need here
    foo: String,
}

pub fn router() -> Router<()> {
    Router::new().route("/", get(self::get::index))
}

mod get {
    use super::*;

    pub async fn index() -> impl IntoResponse {
        FrontpageTemplate {
            foo: "bar".to_string(),
        }.render().unwrap().into_response()
    }
}
