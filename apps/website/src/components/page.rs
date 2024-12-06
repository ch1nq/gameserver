use crate::components::sidebar::Sidebar;
use leptos::*;

#[component]
pub fn Page(children: Children) -> impl IntoView {
    view! {
        <div class="flex flex-col h-screen">
            <div class="flex flex-row h-full">
                <Sidebar />
                <div class="flex flex-col w-full mx-10 mt-4">{children()}</div>
            </div>
        </div>
    }
}
