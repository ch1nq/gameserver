use leptos::prelude::*;

/// 404 Not Found Page
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="flex flex-col ml-10 mt-5 h-screen">
            <h1>"Not found"</h1>
            <p>"This page does not seem to exist"</p>
            <div class="h-4"></div>
            <a href="/" class="hover:font-semibold">
                <i class="bi bi-arrow-left mr-2"></i>
                "Back"
            </a>
        </div>
    }
}
