use maud::{DOCTYPE, Markup, Render, html};

use crate::styles;

pub struct Base<'a> {
    pub title: &'a str,
    pub content: Markup,
    /// Extra `<head>` markup from the host app, e.g. its own stylesheet
    /// `<link>`. Rendered *after* the ui stylesheet link so app styles may
    /// reference the `:root` vars shipped by the library.
    pub head_extra: Markup,
}

/// Zero-JS shell: component styling comes from [`styles::CSS`], which the
/// host app serves at [`styles::PATH`]. Dark mode follows the OS via
/// `prefers-color-scheme` (see the stylesheet).
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
                    link href=(styles::PATH) rel="stylesheet" {}
                    (self.head_extra)
                }
                body {
                    (self.content)
                }
            }
        }
    }
}
