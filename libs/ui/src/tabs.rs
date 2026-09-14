use maud::{Markup, PreEscaped, Render, html};

pub struct Tab<'a> {
    /// Stable slug, e.g. `python`. Used for input/panel ids.
    pub id: &'a str,
    pub label: &'a str,
    pub content: Markup,
}

/// CSS-only tab switcher (no JS): hidden radio inputs drive which
/// panel shows via a small sibling-selector stylesheet.
///
/// Content is caller-provided, so this stays generic — compose with
/// e.g. [`crate::code::CodeBlock`] at the call site.
pub struct Tabs<'a> {
    pub group: &'a str,
    pub tabs: Vec<Tab<'a>>,
}

impl Render for Tabs<'_> {
    fn render(&self) -> Markup {
        let mut css = String::from(".lang-panel{display:none}");
        for tab in &self.tabs {
            css.push_str(&format!(
                "#{g}-{id}:checked~#{g}-bar label[for=\"{g}-{id}\"]{{color:var(--ink);border-bottom-color:var(--ink);font-weight:600}}",
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
                    input type="radio" name=(self.group) id=(format!("{}-{}", self.group, tab.id)) class="sr-only" checked[i == 0] {}
                }
                div id=(format!("{}-bar", self.group)) class="tabs-bar" {
                    @for tab in &self.tabs {
                        label for=(format!("{}-{}", self.group, tab.id)) class="tab-label" {
                            (tab.label)
                        }
                    }
                }
                // NOTE: the panels must stay direct siblings of the radio
                // inputs (no wrapper div): the `:checked ~ #panel-…`
                // selectors below only match following siblings.
                @for tab in &self.tabs {
                    div id=(format!("panel-{}-{}", self.group, tab.id)) class="lang-panel" {
                        (tab.content.clone())
                    }
                }
            }
        }
    }
}
