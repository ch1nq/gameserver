use maud::{Markup, Render, html};

/// Creates a complete table with headers and body rows.
///
/// Stays generic: headers/cells only carry presentation flags (`numeric`,
/// `is_primary`), never domain data. Numeric columns render right-aligned
/// with tabular figures via `.tbl .num`.
pub struct Table<'a> {
    pub headers: Vec<HeaderCell<'a>>,
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
    pub numeric: bool,
}

impl<'a> HeaderCell<'a> {
    pub fn plain(text: &'a str) -> Self {
        Self {
            text,
            numeric: false,
        }
    }

    pub fn numeric(text: &'a str) -> Self {
        Self {
            text,
            numeric: true,
        }
    }
}

impl<'a> From<&'a str> for HeaderCell<'a> {
    fn from(text: &'a str) -> Self {
        Self::plain(text)
    }
}

impl<'a> Render for HeaderCell<'a> {
    fn render(&self) -> Markup {
        html! {
            @if self.numeric {
                th scope="col" class="num" { (self.text) }
            } @else {
                th scope="col" { (self.text) }
            }
        }
    }
}

pub struct Cell {
    pub content: Markup,
    pub is_primary: bool,
    pub numeric: bool,
}

impl Cell {
    pub fn plain(content: Markup) -> Self {
        Self {
            content,
            is_primary: false,
            numeric: false,
        }
    }

    pub fn primary(content: Markup) -> Self {
        Self {
            content,
            is_primary: true,
            numeric: false,
        }
    }

    pub fn numeric(content: Markup) -> Self {
        Self {
            content,
            is_primary: false,
            numeric: true,
        }
    }

    pub fn numeric_primary(content: Markup) -> Self {
        Self {
            content,
            is_primary: true,
            numeric: true,
        }
    }
}

impl Render for Cell {
    fn render(&self) -> Markup {
        html! {
            @match (self.numeric, self.is_primary) {
                (true, true) => {
                    td class="num primary" { (self.content) }
                }
                (true, false) => {
                    td class="num" { (self.content) }
                }
                (false, true) => {
                    td class="primary" { (self.content) }
                }
                (false, false) => {
                    td { (self.content) }
                }
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
