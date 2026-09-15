use crate::users::{AuthSession, User};
use achtung_ui::error::Error;
use maud::{Markup, Render, html};

// Re-export components from the shared library for convenience.
// `achtung-ui` stays standalone (only `maud`): apps compose these
// primitives with domain data, never the other way around.
pub use achtung_ui::Icon;
pub use achtung_ui::alert;
pub use achtung_ui::avatar;
pub use achtung_ui::button;
pub use achtung_ui::form;
pub use achtung_ui::modal;
pub use achtung_ui::section;
pub use achtung_ui::table;

pub struct Page<'a> {
    pub title: &'a str,
    pub content: Markup,
    pub session: &'a AuthSession,
    pub errors: Vec<Error>,
}

impl Page<'_> {
    pub fn with_errors(mut self, errors: Vec<Error>) -> Self {
        self.errors.extend(errors);
        self
    }
}

impl<'a> Render for Page<'a> {
    fn render(&self) -> Markup {
        achtung_ui::base::Base {
            title: self.title,
            // App stylesheet after the ui stylesheet (see `achtung_ui::styles`):
            // page styles may use the library's `:root` vars.
            head_extra: html! {
                link href="/static/app.css" rel="stylesheet" {}
            },
            content: html! {
                (Navbar { session: self.session })
                div class="page" {
                    @for error in &self.errors {
                        (error)
                    }
                    (self.content)
                }
            },
        }
        .render()
    }
}

struct UserDropdown<'a> {
    user: &'a User,
}

impl<'a> Render for UserDropdown<'a> {
    fn render(&self) -> Markup {
        html! {
            details class="dropdown" {
                summary class="dropdown-toggle" {
                    span class="sr-only" { "Open user menu" }
                    img src=(avatar::github_avatar_url_default(&self.user.username)) alt="user photo";
                    (&*self.user.username)
                    svg class="icon" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6" {
                        path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4";
                    }
                }
                div class="dropdown-menu" {
                    ul {
                        li {
                            a href="/agents" { "Manage agents" }
                        }
                        li {
                            a href="/settings" { "Settings" }
                        }
                    }
                    div class="dropdown-div" {
                        a href="/logout" { "Sign out" }
                    }
                }
            }
        }
    }
}

pub struct Navbar<'a> {
    pub session: &'a AuthSession,
}

/// Brand mark for the navbar. Lives in the website (not `achtung-ui`)
/// because it is product-specific, not a reusable primitive.
pub struct AchtungLogo;

impl Render for AchtungLogo {
    fn render(&self) -> Markup {
        html! {
            svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true" class="brand-mark" {
                rect x="0.5" y="0.5" width="23" height="23" rx="4" fill="#0A0B10" {}
                path d="M4 18 C 8 18, 7 6, 12 6 C 17 6, 16 16, 20 13" stroke-width="2.4" fill="none" stroke-linecap="round" style="stroke:var(--brand);" {}
                circle cx="20" cy="13" r="1.9" style="fill:var(--yellow);" {}
            }
        }
    }
}

impl<'a> Render for Navbar<'a> {
    fn render(&self) -> Markup {
        html! {
            nav class="navbar" {
                div class="navbar-inner" {
                    a href="/" class="brand" {
                        (AchtungLogo)
                        span class="brand-name" {
                            "Achtung, die Bots"
                        }
                    }
                    div class="nav-links" {
                        a href="#board" class="nav-link" { "Leaderboard" }
                        a href="#bot-file" class="nav-link" { "Docs" }
                        a href="https://github.com/ch1nq/gameserver" class="nav-link" {
                            (Icon::GithubLogo)
                            "Star"
                        }
                        @if let Some(user) = &self.session.user {
                            (UserDropdown { user });
                        }
                        @else {
                            a href="/login" class="signin-btn" {
                                "Sign in"
                            }
                        }
                    }
                }
            }
        }
    }
}
