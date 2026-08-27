use maud::{DOCTYPE, Markup, Render, html};

pub struct Base<'a> {
    pub title: &'a str,
    pub content: Markup,
}

impl<'a> Render for Base<'a> {
    fn render(&self) -> Markup {
        html! {
            (DOCTYPE)
            html {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (self.title) }
                    link href="https://cdn.jsdelivr.net/npm/flowbite@4.0.1/dist/flowbite.min.css" rel="stylesheet";
                    script src="https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4" {}
                }
                body class="bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-white" {
                    (self.content)
                    script src="https://cdn.jsdelivr.net/npm/flowbite@4.0.1/dist/flowbite.min.js" {};
                    script src="https://cdn.jsdelivr.net/npm/flowbite@4.0.1/dist/flowbite.min.js" {};
                }
            }
        }
    }
}
