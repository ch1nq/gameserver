use maud::{DOCTYPE, Markup, Render, html};

pub struct Base<'a> {
    pub title: &'a str,
    pub content: Markup,
}

/// Zero-JS shell: all styling lives in `/static/app.css` (theme vars +
/// semantic classes). Dark mode follows the OS via `prefers-color-scheme`.
impl<'a> Render for Base<'a> {
    fn render(&self) -> Markup {
        html! {
            (DOCTYPE)
            html {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (self.title) }
                    link rel="preconnect" href="https://fonts.googleapis.com" {}
                    link rel="preconnect" href="https://fonts.gstatic.com" crossorigin {}
                    link href="https://fonts.googleapis.com/css2?family=Geologica:wght,CRSV@100..900,0&display=swap" rel="stylesheet" {}
                    link href="/static/app.css" rel="stylesheet" {}
                }
                body {
                    (self.content)
                }
            }
        }
    }
}
