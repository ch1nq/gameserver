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
            span class="avatar" {
                span class="avatar-fallback" {
                    (self.fallback)
                }
                @if let Some(src) = self.src {
                    img src=(src) alt="" loading="lazy" onerror="this.remove()" class="avatar-img" {}
                }
            }
        }
    }
}
