use leptos_router::StaticSegment;

pub mod agents;
pub mod home;
pub mod live_battle;
pub mod not_found;
pub mod settings;
pub mod stats;

pub struct PageMeta {
    pub title: &'static str,
    pub path: StaticSegment<&'static str>,
}

#[derive(PartialEq)]
pub enum Page {
    LiveBattle,
    Stats,
    Agents,
    Settings,
    Logout,
}

pub fn get_page_meta(page: Page) -> PageMeta {
    match page {
        Page::LiveBattle => PageMeta {
            title: "Live battle",
            path: StaticSegment("/"),
        },
        Page::Stats => PageMeta {
            title: "Stats",
            path: StaticSegment("/stats"),
        },
        Page::Agents => PageMeta {
            title: "Manage agents",
            path: StaticSegment("/agents"),
        },
        Page::Settings => PageMeta {
            title: "Settings",
            path: StaticSegment("/settings"),
        },
        Page::Logout => PageMeta {
            title: "Logout",
            path: StaticSegment("/logout"),
        },
    }
}
