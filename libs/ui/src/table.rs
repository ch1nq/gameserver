use maud::{Markup, Render, html};

/// Creates a complete table with headers and body rows
pub struct Table<'a> {
    pub headers: Vec<&'a str>,
    pub rows: Markup,
    pub extra_classes: Option<&'a str>,
}

impl<'a> Render for Table<'a> {
    fn render(&self) -> Markup {
        // Kept for back-compat; prefer semantic `.tbl-wrap` styling.
        let _ = self.extra_classes;

        let headers = self
            .headers
            .iter()
            .map(|h| HeaderCell { text: h })
            .fold(html! {}, |acc, h| html! { (acc) (h) });

        html! {
            div class="tbl-wrap" {
                table class="tbl" {
                    thead {
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
            th scope="col" { (self.text) }
        }
    }
}

pub struct Cell {
    pub content: Markup,
    pub is_primary: bool,
}

impl Render for Cell {
    fn render(&self) -> Markup {
        html! {
            @if self.is_primary {
                td class="primary" { (self.content) }
            } @else {
                td { (self.content) }
            }
        }
    }
}

pub struct Row {
    pub content: Markup,
}

impl Render for Row {
    fn render(&self) -> Markup {
        html! {
            tr {
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
            tr {
                td colspan=(self.colspan) class="center" {
                    (self.message)
                }
            }
        }
    }
}
