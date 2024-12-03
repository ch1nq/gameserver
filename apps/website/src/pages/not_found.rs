use leptos::*;

/// 404 Not Found Page
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <h1>"Not found"</h1>
        <h2>"This page does not seem to exist"</h2>
        <a href="/">"Home"</a>
    }
}
