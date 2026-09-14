use maud::{Markup, Render, html};

/// Monospace code panel, mirrors the mockup `surface-2` block.
pub struct CodeBlock<'a> {
    pub code: &'a str,
}

impl Render for CodeBlock<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="code" {
                pre { (self.code) }
            }
        }
    }
}
