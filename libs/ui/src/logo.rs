use maud::{Markup, Render, html};

/// Wordmark logo from the landing mockup: black rounded square,
/// pink curve and yellow dot.
pub struct AchtungLogo;

impl Render for AchtungLogo {
    fn render(&self) -> Markup {
        html! {
            svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true" class="block flex-none" {
                rect x="0.5" y="0.5" width="23" height="23" rx="4" fill="#0A0B10" {}
                path d="M4 18 C 8 18, 7 6, 12 6 C 17 6, 16 16, 20 13" stroke="#e0338a" stroke-width="2.4" fill="none" stroke-linecap="round" {}
                circle cx="20" cy="13" r="1.9" fill="#ffd84a" {}
            }
        }
    }
}

/// Small Github mark used by the navbar Star link.
pub struct GithubMark;

impl Render for GithubMark {
    fn render(&self) -> Markup {
        html! {
            svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" class="block flex-none" {
                path d="M8 .2a8 8 0 0 0-2.53 15.59c.4.07.55-.17.55-.38l-.01-1.49c-2.22.48-2.69-1.07-2.69-1.07-.36-.92-.89-1.17-.89-1.17-.72-.5.06-.49.06-.49.8.06 1.22.82 1.22.82.71 1.22 1.87.87 2.33.66.07-.52.28-.87.5-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 4 0c1.53-1.03 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.28.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48l-.01 2.2c0 .21.14.46.55.38A8 8 0 0 0 8 .2Z" {}
            }
        }
    }
}
