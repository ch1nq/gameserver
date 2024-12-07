use crate::components::sidebar::Sidebar;
use leptos::*;

#[component]
pub fn Page(children: Children) -> impl IntoView {
    view! {
        <div class="flex flex-col h-screen">
            <div class="flex flex-row-reverse h-full">
                <div class="flex flex-col w-full px-5 pt-4 bg-gray-200 overflow-y-scroll">{children()}</div>
                <Sidebar />
            </div>
        </div>
    }
}
