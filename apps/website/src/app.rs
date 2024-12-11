use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes};

use crate::components::sidebar::Sidebar;
use crate::pages;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
            // injects a stylesheet into the document <head>
            // id=leptos means cargo-leptos will hot-reload this stylesheet
            <Stylesheet id="leptos" href="/pkg/website.css"/>

            // sets the document title
            <Title text="Welcome to Leptos"/>

            // content for this welcome page
            <Router>
                <Routes fallback=pages::not_found::NotFound>
                    <Route
                        path=pages::get_page_meta(pages::Page::LiveBattle).path
                        view=|| page_wrapper(pages::live_battle::LiveBattle, pages::Page::LiveBattle)
                    />
                    <Route
                        path=pages::get_page_meta(pages::Page::Stats).path
                        view=|| page_wrapper(pages::stats::Stats, pages::Page::Stats)
                    />
                    <Route
                        path=pages::get_page_meta(pages::Page::Agents).path
                        view=|| page_wrapper(pages::agents::Agents, pages::Page::Agents)
                    />
                    <Route
                        path=pages::get_page_meta(pages::Page::Settings).path
                        view=|| page_wrapper(pages::settings::Settings, pages::Page::Settings)
                    />
                </Routes>
        </Router>
    }
}

fn page_wrapper(content: impl IntoView, current_page: pages::Page) -> impl IntoView {
    view! {
        <div class="h-screen">
            <div class="flex flex-row-reverse h-full">
                <main class="flex flex-col w-full px-5 pt-4 bg-gray-200 overflow-y-scroll">
                    {content}
                </main>
                <Sidebar current_page=current_page />
            </div>
        </div>
    }
}
