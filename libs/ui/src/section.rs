use maud::{Markup, Render, html};

/// Generic content section wrapper (`section.section`).
/// Standalone: only id + markup, no domain types.
pub struct Section<'a> {
    pub id: Option<&'a str>,
    pub content: Markup,
}

impl Render for Section<'_> {
    fn render(&self) -> Markup {
        html! {
            @if let Some(id) = self.id {
                section id=(id) class="section" {
                    (self.content)
                }
            } @else {
                section class="section" {
                    (self.content)
                }
            }
        }
    }
}

/// Section heading row: title + optional trailing meta (e.g. counts).
pub struct SectionHead {
    pub title: Markup,
    pub sub: Option<Markup>,
}

impl Render for SectionHead {
    fn render(&self) -> Markup {
        html! {
            div class="section-head" {
                h2 class="section-title" {
                    (self.title)
                }
                @if let Some(sub) = &self.sub {
                    span class="section-sub" {
                        (sub)
                    }
                }
            }
        }
    }
}

/// Intro paragraph (`p.lede`). Content is caller-provided markup.
pub struct Lede {
    pub content: Markup,
}

impl Render for Lede {
    fn render(&self) -> Markup {
        html! {
            p class="lede" {
                (self.content)
            }
        }
    }
}

/// Small muted footnote (`p.note`). Content is caller-provided so links
/// and inline markup keep working.
pub struct Note {
    pub content: Markup,
}

impl Render for Note {
    fn render(&self) -> Markup {
        html! {
            p class="note" {
                (self.content)
            }
        }
    }
}

/// Horizontal action row (e.g. primary button + accent link).
pub struct ActionsRow {
    pub content: Markup,
}

impl Render for ActionsRow {
    fn render(&self) -> Markup {
        html! {
            div class="actions-row" {
                (self.content)
            }
        }
    }
}
