use leptos::*;

use crate::pages::{get_page_meta, Page, PageMeta};

struct SidebarEntry {
    icon: &'static str,
    page_meta: PageMeta,
}

impl IntoView for SidebarEntry {
    fn into_view(self) -> View {
        let active = false;
        let base_class = "block py-2 px-4 text-gray-200 hover:bg-primary hover:text-white";
        let active_class = if active { "font-semibold" } else { "" };
        view! {
            <a href=self.page_meta.path class=format!("{} {}", base_class, active_class)>
                <i class=format!(
                    "text-xl mr-3 bi {}{}",
                    self.icon,
                    if active { "-fill" } else { "" },
                )></i>
                {self.page_meta.title}
            </a>
        }
        .into_view()
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let top_entries = vec![
        SidebarEntry {
            icon: "bi-tv",
            page_meta: get_page_meta(Page::LiveBattle),
        },
        SidebarEntry {
            icon: "bi-bar-chart",
            page_meta: get_page_meta(Page::Stats),
        },
        SidebarEntry {
            icon: "bi-diagram-3",
            page_meta: get_page_meta(Page::Agents),
        },
    ];
    let bottom_entries = vec![
        SidebarEntry {
            icon: "bi-gear",
            page_meta: get_page_meta(Page::Settings),
        },
        SidebarEntry {
            icon: "bi-box-arrow-right",
            page_meta: get_page_meta(Page::Logout),
        },
    ];
    view! {
        <aside>
            <nav class="bg-gray-900 w-64 h-full">
                <div class=" flex flex-col justify-between py-2 h-full">
                    <div class="flex flex-col">{top_entries.collect_view()}</div>
                    <div class="flex flex-col">{bottom_entries.collect_view()}</div>
                </div>
            </nav>
        </aside>
    }
}
