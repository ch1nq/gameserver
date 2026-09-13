use maud::{Markup, Render, html};

/// Small round avatar with a text fallback underneath.
///
/// If `src` is `Some` and the image fails to load, `onerror` removes it
/// so the fallback stays readable. Knows nothing about users or providers.
pub struct Avatar<'a> {
    pub src: Option<&'a str>,
    pub fallback: &'a str,
}

impl Render for Avatar<'_> {
    fn render(&self) -> Markup {
        html! {
            span class="relative block w-5 h-5 flex-none" {
                span class="absolute inset-0 rounded-full bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-300 text-[10px] font-bold flex items-center justify-center" {
                    (self.fallback)
                }
                @if let Some(src) = self.src {
                    img src=(src) alt="" loading="lazy" onerror="this.remove()" class="relative w-5 h-5 rounded-full block object-cover" {}
                }
            }
        }
    }
}
