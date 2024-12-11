use leptos::prelude::*;

use crate::pages::{get_page_meta, Page, PageMeta};

struct SidebarEntry {
    icon: &'static str,
    page_meta: PageMeta,
    is_active: bool,
}

#[component]
fn SidebarEntryComponent(entry: SidebarEntry) -> impl IntoView {
    let base_class = "flex flex-col items-center
        pt-1 pb-2
        text-gray-800
        block
        hover:bg-gray-200
        hover:font-bold
    ";
    let active_class = if entry.is_active { "font-semibold" } else { "" };
    view! {
        <a href=entry.page_meta.path.0 class=format!("{} {}", base_class, active_class)>
            <i class=format!(
                "text-xl bi {}{}",
                entry.icon,
                if entry.is_active { "-fill" } else { "" },
            )></i>
            <span class="text-xs text-center
            ">{entry.page_meta.title}</span>
        </a>
    }
}

#[component]
pub fn Sidebar(current_page: Page) -> impl IntoView {
    let top_entries = vec![
        SidebarEntry {
            icon: "bi-tv",
            page_meta: get_page_meta(Page::LiveBattle),
            is_active: current_page == Page::LiveBattle,
        },
        SidebarEntry {
            icon: "bi-bar-chart",
            page_meta: get_page_meta(Page::Stats),
            is_active: current_page == Page::Stats,
        },
        SidebarEntry {
            icon: "bi-diagram-3",
            page_meta: get_page_meta(Page::Agents),
            is_active: current_page == Page::Agents,
        },
    ];
    let bottom_entries = vec![
        SidebarEntry {
            icon: "bi-gear",
            page_meta: get_page_meta(Page::Settings),
            is_active: current_page == Page::Settings,
        },
        SidebarEntry {
            icon: "bi-box-arrow-right",
            page_meta: get_page_meta(Page::Logout),
            is_active: false,
        },
    ];
    view! {
        <aside>
            <nav class="bg-gray-100 w-20">
                <div class=" flex flex-col justify-between py-2 h-screen shadow-md">
                    <div class="flex flex-col">{top_entries.into_iter().map(|entry| view!{<SidebarEntryComponent entry=entry />}).collect_view()}</div>
                    <div class="flex flex-col">{bottom_entries.into_iter().map(|entry| view!{<SidebarEntryComponent entry=entry />}).collect_view()}</div>
                </div>
            </nav>
        </aside>
    }
}
