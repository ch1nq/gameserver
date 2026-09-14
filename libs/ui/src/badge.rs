use maud::{Markup, Render, html};

/// Small pill with a pulsing dot, e.g. a `Live` indicator.
///
/// Styling is fixed; the label is caller-provided so this stays generic.
pub struct Badge<'a> {
    pub label: &'a str,
}

impl Render for Badge<'_> {
    fn render(&self) -> Markup {
        html! {
            span class="badge" {
                span class="badge-dot" {}
                (self.label)
            }
        }
    }
}
