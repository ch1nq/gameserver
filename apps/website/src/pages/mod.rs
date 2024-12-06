pub mod agents;
pub mod home;
pub mod live_battle;
pub mod not_found;
pub mod settings;
pub mod stats;

pub struct PageMeta {
    pub title: &'static str,
    pub path: &'static str,
}

pub enum Page {
    Home,
    LiveBattle,
    Stats,
    Agents,
    Settings,
    Logout,
    NotFound,
}

pub fn get_page_meta(page: Page) -> PageMeta {
    match page {
        Page::Home => PageMeta {
            title: "Home",
            path: "/",
        },
        Page::LiveBattle => PageMeta {
            title: "Live battle",
            path: "/live",
        },
        Page::Stats => PageMeta {
            title: "Stats",
            path: "/stats",
        },
        Page::Agents => PageMeta {
            title: "Manage agents",
            path: "/agents",
        },
        Page::Settings => PageMeta {
            title: "Settings",
            path: "/settings",
        },
        Page::Logout => PageMeta {
            title: "Logout",
            path: "/logout",
        },
        Page::NotFound => PageMeta {
            title: "Not found",
            path: "/*",
        },
    }
}
