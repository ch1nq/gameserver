use maud::{Markup, Render, html};

/// Creates a complete table with headers and body rows
pub struct Table<'a> {
    pub headers: Vec<&'a str>,
    pub rows: Markup,
    pub extra_classes: Option<&'a str>,
}

impl<'a> Render for Table<'a> {
    fn render(&self) -> Markup {
        let wrapper_class = if let Some(extra) = self.extra_classes {
            format!("relative overflow-x-auto {}", extra)
        } else {
            "relative overflow-x-auto".to_string()
        };

        let headers = self
            .headers
            .iter()
            .map(|h| HeaderCell { text: h })
            .fold(html! {}, |acc, h| html! { (acc) (h) });

        html! {
            div class=(wrapper_class) {
                table class="w-full text-sm text-left rtl:text-right text-[var(--muted)]" {
                    thead class="text-xs uppercase bg-[var(--surface-2)] text-[var(--muted)]" {
                        tr {(headers)}
                    }
                    tbody {(self.rows)}
                }
            }
        }
    }
}

pub struct HeaderCell<'a> {
    pub text: &'a str,
}

impl<'a> Render for HeaderCell<'a> {
    fn render(&self) -> Markup {
        html! {
            th scope="col" class="px-6 py-3" { (self.text) }
        }
    }
}

pub struct Cell {
    pub content: Markup,
    pub is_primary: bool,
}

impl Render for Cell {
    fn render(&self) -> Markup {
        let class = if self.is_primary {
            "px-6 py-4 font-medium whitespace-nowrap text-[var(--ink)]"
        } else {
            "px-6 py-4"
        };

        html! {
            td class=(class) { (self.content) }
        }
    }
}

pub struct Row {
    pub content: Markup,
}

impl Render for Row {
    fn render(&self) -> Markup {
        html! {
            tr class="bg-[var(--surface)] border-b border-[var(--line-soft)]" {
                (self.content)
            }
        }
    }
}

pub struct EmptyRow<'a> {
    pub colspan: usize,
    pub message: &'a str,
}

impl<'a> Render for EmptyRow<'a> {
    fn render(&self) -> Markup {
        html! {
            tr class="bg-[var(--surface)] border-b border-[var(--line-soft)]" {
                td colspan=(self.colspan) class="px-6 py-4 text-center text-[var(--muted)]" {
                    (self.message)
                }
            }
        }
    }
}
