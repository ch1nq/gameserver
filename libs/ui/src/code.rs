use maud::{Markup, PreEscaped, Render, html};

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

pub struct CodeTab<'a> {
    /// Stable slug, e.g. `python`. Used for input/panel ids.
    pub id: &'a str,
    pub label: &'a str,
    pub code: &'a str,
    pub install: &'a str,
}

/// CSS-only tab switcher (no JS): hidden radio inputs drive which
/// panel shows via a small sibling-selector stylesheet.
pub struct CodeTabs<'a> {
    pub group: &'a str,
    pub tabs: Vec<CodeTab<'a>>,
}

impl<'a> Render for CodeTabs<'a> {
    fn render(&self) -> Markup {
        let mut css = String::from(".lang-panel{display:none}");
        for tab in &self.tabs {
            css.push_str(&format!(
                "#{g}-{id}:checked~#{g}-bar label[for=\"{g}-{id}\"]{{color:#111827;border-bottom-color:#111827;font-weight:600}}",
                g = self.group,
                id = tab.id
            ));
            css.push_str(&format!(
                "@media (prefers-color-scheme:dark){{#{g}-{id}:checked~#{g}-bar label[for=\"{g}-{id}\"]{{color:#fff;border-bottom-color:#fff}}}}",
                g = self.group,
                id = tab.id
            ));
            css.push_str(&format!(
                "#{g}-{id}:checked~#panel-{g}-{id}{{display:block}}",
                g = self.group,
                id = tab.id
            ));
        }
        html! {
            div {
                style { (PreEscaped(css)) }
                @for (i, tab) in self.tabs.iter().enumerate() {
                    input type="radio" name=(self.group) id=(format!("{}-{}", self.group, tab.id)) class="hidden" checked[i == 0] {}
                }
                div id=(format!("{}-bar", self.group)) class="flex gap-0.5 border-b border-gray-300 dark:border-gray-700 max-w-[760px]" {
                    @for tab in &self.tabs {
                        label for=(format!("{}-{}", self.group, tab.id)) class="cursor-pointer bg-transparent border-b-2 border-transparent text-gray-500 dark:text-gray-400 text-sm font-medium px-3.5 py-3 -mb-px" {
                            (tab.label)
                        }
                    }
                }
                // NOTE: the panels must stay direct siblings of the radio
                // inputs (no wrapper div): the `:checked ~ #panel-…`
                // selectors below only match following siblings.
                @for tab in &self.tabs {
                    div id=(format!("panel-{}-{}", self.group, tab.id)) class="lang-panel pt-4" {
                        (CodeBlock { code: tab.code })
                        div class="flex flex-col gap-2 pt-4" {
                            span class="text-[13px] text-gray-500 dark:text-gray-400" {
                                "Install the CLI once, then build and push the image:"
                            }
                            (CodeBlock { code: tab.install })
                        }
                    }
                }
            }
        }
    }
}
