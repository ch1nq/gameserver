use components::sidebar::Sidebar;
use leptos::*;
use leptos_meta::*;
use leptos_router::*;
use pages::get_page_meta;

// Modules
mod components;
mod pages;

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
                <Route
                    path=get_page_meta(pages::Page::LiveBattle).path
                    view=|| page_wrapper(pages::live_battle::LiveBattle, pages::Page::LiveBattle)
                />
                <Route
                    path=get_page_meta(pages::Page::Stats).path
                    view=|| page_wrapper(pages::stats::Stats, pages::Page::Stats)
                />
                <Route
                    path=get_page_meta(pages::Page::Agents).path
                    view=|| page_wrapper(pages::agents::Agents, pages::Page::Agents)
                />
                <Route
                    path=get_page_meta(pages::Page::Settings).path
                    view=|| page_wrapper(pages::settings::Settings, pages::Page::Settings)
                />
                <Route
                    path=get_page_meta(pages::Page::NotFound).path
                    view=pages::not_found::NotFound
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
