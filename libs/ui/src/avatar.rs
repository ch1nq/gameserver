use common::Username;
use maud::{Markup, Render, html};

/// Author avatar with initial fallback. If the github image 404s,
/// `onerror` removes it so the initial underneath stays readable
/// (same pattern as the landing mockup).
pub struct AuthorAvatar<'a> {
    pub username: &'a Username,
}

impl Render for AuthorAvatar<'_> {
    fn render(&self) -> Markup {
        let initial = self
            .username
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string());
        let src = format!("https://github.com/{}.png?size=40", self.username);
        html! {
            span class="relative block w-5 h-5 flex-none" {
                span class="absolute inset-0 rounded-full bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-300 text-[10px] font-bold flex items-center justify-center" {
                    (initial)
                }
                img src=(src) alt="" loading="lazy" onerror="this.remove()" class="relative w-5 h-5 rounded-full block object-cover" {}
            }
        }
    }
}
