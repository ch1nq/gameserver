use maud::{Markup, Render, html};

/// Pink `LIVE` pill from the landing mockup.
pub struct LiveBadge;

impl Render for LiveBadge {
    fn render(&self) -> Markup {
        html! {
            span class="inline-flex items-center gap-1.5 bg-[#e0338a] dark:bg-[#ff59a3] text-white dark:text-gray-900 font-bold text-[11px] tracking-[0.1em] uppercase px-2.5 py-1 rounded" {
                span class="block w-[7px] h-[7px] rounded-full bg-current animate-pulse" {}
                "Live"
            }
        }
    }
}
