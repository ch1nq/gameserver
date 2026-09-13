use maud::{Markup, Render, html};

/// Monospace code panel, mirrors the mockup `surface-2` block.
pub struct CodeBlock<'a> {
    pub code: &'a str,
}

impl Render for CodeBlock<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="bg-gray-50 dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded p-4 max-w-[760px] overflow-x-auto font-mono text-[13.5px] leading-[1.75] text-gray-700 dark:text-gray-300" {
                pre class="whitespace-pre m-0" { (self.code) }
            }
        }
    }
}
