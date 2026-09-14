use crate::users::{AuthSession, User};
use achtung_ui::error::Error;
use maud::{Markup, Render, html};

// Re-export components from the shared library for convenience
pub use achtung_ui::Icon;
pub use achtung_ui::alert;
pub use achtung_ui::button;
pub use achtung_ui::form;
pub use achtung_ui::modal;
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
            content: html! {
                (Navbar { session: self.session })
                div class="mx-auto w-full max-w-[1280px] px-7 pt-[26px] pb-[70px]" {
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

pub fn profile_picture_url(user: &User) -> String {
    format!("https://github.com/{}.png", user.username)
}

struct UserDropdown<'a> {
    user: &'a User,
}

impl<'a> Render for UserDropdown<'a> {
    fn render(&self) -> Markup {
        html! {
            button id="dropdownAvatarNameButton" data-dropdown-toggle="dropdownAvatarName" class="flex items-center text-sm pe-1 font-medium rounded-full text-[var(--ink)] hover:text-[var(--accent)] md:me-0 focus:ring-4 focus:ring-gray-100" type="button" {
                span class="sr-only" { "Open user menu" }
                    img class="w-8 h-8 me-2 rounded-full" src=(profile_picture_url(self.user)) alt="user photo";
                    (&*self.user.username)
                    svg class="w-2.5 h-2.5 ms-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6" {
                        path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4";
                    }
                }

            div id="dropdownAvatarName" class="z-10 hidden divide-y divide-[var(--line-soft)] rounded-lg shadow-sm w-44 bg-[var(--surface)]" {
                ul class="py-2 text-sm text-[var(--mid)]" aria-labelledby="dropdownInformdropdownAvatarNameButtonationButton" {
                    li {
                        a href="/agents" class="block px-4 py-2 hover:bg-[var(--hover)] hover:text-[var(--ink)]" { "Manage agents" }
                    }
                    li {
                        a href="/settings" class="block px-4 py-2 hover:bg-[var(--hover)] hover:text-[var(--ink)]" { "Settings" }
                    }
                }
                div class="py-2" {
                    a href="/logout" class="block px-4 py-2 text-sm text-[var(--mid)] hover:bg-[var(--hover)] hover:text-[var(--ink)]" { "Sign out" }
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
            svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true" class="block flex-none" {
                rect x="0.5" y="0.5" width="23" height="23" rx="4" fill="#0A0B10" {}
                path d="M4 18 C 8 18, 7 6, 12 6 C 17 6, 16 16, 20 13" stroke-width="2.4" fill="none" stroke-linecap="round" style="stroke:var(--brand);" {}
                circle cx="20" cy="13" r="1.9" style="fill:var(--yellow);" {}
            }
        }
    }
}

impl<'a> Render for Navbar<'a> {
    fn render(&self) -> Markup {
        // Mockup tokens: muted links, hover fill + ink, 3px radius.
        let link = "text-[14px] font-semibold text-[var(--muted)] hover:text-[var(--ink)] hover:bg-[var(--hover)] px-2.5 py-2 rounded-[3px]";
        html! {
            nav class="border-b border-[var(--line)]" {
                div class="mx-auto w-full max-w-[1280px] px-7 flex items-center gap-[18px] flex-wrap py-2.5" {
                    a href="/" class="flex items-center gap-2.5" style="color:var(--ink);" {
                        (AchtungLogo)
                        span class="font-[Geologica] font-semibold text-[19px] tracking-[-0.02em] text-[var(--ink)]" {
                            "Achtung, die Bots"
                        }
                    }
                    div class="ml-auto flex items-center gap-2 flex-wrap" {
                        a href="#board" class=(link) { "Leaderboard" }
                        a href="#bot-file" class=(link) { "Docs" }
                        a href="https://github.com" class=(format!("{link} inline-flex items-center gap-1.5")) {
                            (Icon::GithubLogo)
                            "Star"
                        }
                        (ThemeToggle)
                        @if let Some(user) = &self.session.user {
                            (UserDropdown { user });
                        }
                        @else {
                            a href="/login" class="text-sm font-semibold px-3.5 py-2 rounded-[3px] text-[var(--invert-ink)] bg-[var(--invert-bg)] hover:bg-[var(--accent)]" style="color:var(--invert-ink);" {
                                "Sign in"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Manual light/dark switch from the mockup. Flips `html[data-theme]`
/// (see `achtung_ui::base`) and persists to localStorage.
pub struct ThemeToggle;

impl Render for ThemeToggle {
    fn render(&self) -> Markup {
        html! {
            button
                type="button"
                data-theme-toggle=""
                onclick="toggleAchtungTheme()"
                aria-label="Switch to dark theme"
                title="Switch to dark theme"
                class="flex items-center justify-center w-8 h-8 flex-none rounded-[3px] text-[var(--muted)] hover:text-[var(--ink)] hover:bg-[var(--hover)]"
                style="appearance:none;cursor:pointer;background:transparent;border:none;" {
                span data-icon-moon="" {
                    svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" class="block" {
                        path d="M13.3 10.6A5.8 5.8 0 0 1 5.4 2.7a5.8 5.8 0 1 0 7.9 7.9Z" {}
                    }
                }
                span data-icon-sun="" {
                    svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true" class="block" {
                        circle cx="8" cy="8" r="3" {}
                        path d="M8 1.1v1.5M8 13.4v1.5M1.1 8h1.5M13.4 8h1.5M3.2 3.2l1.1 1.1M11.7 11.7l1.1 1.1M12.8 3.2l-1.1 1.1M4.3 11.7l-1.1 1.1" stroke-linecap="round" {}
                    }
                }
            }
        }
    }
}
