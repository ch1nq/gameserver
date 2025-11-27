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
                table class="w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400" {
                    thead class="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400" {
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
            "px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white"
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
            tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
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
            tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                td colspan=(self.colspan) class="px-6 py-4 text-center text-gray-500 dark:text-gray-400" {
                    (self.message)
                }
            }
        }
    }
}
