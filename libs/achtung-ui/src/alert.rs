use maud::{Markup, Render, html};

pub struct Warning<'a> {
    pub title: &'a str,
    pub message: &'a str,
}

impl<'a> Render for Warning<'a> {
    fn render(&self) -> Markup {
        html! {
            div class="p-4 mb-4 text-sm text-yellow-800 rounded-lg bg-yellow-50 dark:bg-gray-800 dark:text-yellow-300" role="alert" {
                span class="font-medium" { (self.title) }
                " " (self.message)
            }
        }
    }
}

pub struct Info {
    pub content: Markup,
}

impl Render for Info {
    fn render(&self) -> Markup {
        html! {
            div class="p-4 text-sm text-gray-800 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-300" {
                (self.content)
            }
        }
    }
}
