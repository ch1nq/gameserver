use leptos::*;
use leptos_meta::*;
use leptos_router::*;
use pages::get_page_meta;

// Modules
mod components;
mod pages;

// Top-Level pages
use crate::pages::home::Home;
use crate::pages::not_found::NotFound;

/// An app router which renders the homepage and handles 404's
#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <Html lang="en" dir="ltr" attr:data-theme="light" />

        // sets the document title
        <Title text="Achtung battle" />

        // injects metadata in the <head> of the page
        <Meta charset="UTF-8" />
        <Meta name="viewport" content="width=device-width, initial-scale=1.0" />

        <Router>
            <Routes>
                <Route path=get_page_meta(pages::Page::Home).path view=Home />
                <Route
                    path=get_page_meta(pages::Page::LiveBattle).path
                    view=pages::live_battle::LiveBattle
                />
                <Route path=get_page_meta(pages::Page::Stats).path view=pages::stats::Stats />
                <Route path=get_page_meta(pages::Page::Agents).path view=pages::agents::Agents />
                <Route
                    path=get_page_meta(pages::Page::Settings).path
                    view=pages::settings::Settings
                />
                <Route path=get_page_meta(pages::Page::NotFound).path view=NotFound />
            </Routes>
        </Router>
    }
}
