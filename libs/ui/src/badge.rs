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
            span class="inline-flex items-center gap-[7px] bg-[var(--brand)] text-[var(--brand-ink)] font-bold text-[11px] tracking-[0.1em] uppercase px-[9px] py-1 rounded-[3px]" style="color:var(--brand-ink);" {
                span class="block w-[7px] h-[7px] rounded-full bg-current animate-pulse" {}
                (self.label)
            }
        }
    }
}
