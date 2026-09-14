use maud::{Markup, Render, html};

/// Monospace code panel, mirrors the mockup `surface-2` block.
pub struct CodeBlock<'a> {
    pub code: &'a str,
}

impl Render for CodeBlock<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="bg-[var(--surface-2)] border border-[var(--line)] rounded px-[18px] py-4 max-w-[760px] overflow-x-auto font-mono text-[13.5px] leading-[1.75] text-[var(--mid)]" {
                pre class="whitespace-pre m-0" { (self.code) }
            }
        }
    }
}
