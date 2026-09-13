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
                div class="mx-auto w-full max-w-[1280px] px-7 pt-6 pb-16" {
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
            button id="dropdownAvatarNameButton" data-dropdown-toggle="dropdownAvatarName" class="flex items-center text-sm pe-1 font-medium text-gray-900 rounded-full hover:text-blue-600 dark:hover:text-blue-500 md:me-0 focus:ring-4 focus:ring-gray-100 dark:focus:ring-gray-700 dark:text-white" type="button" {
                span class="sr-only" { "Open user menu" }
                    img class="w-8 h-8 me-2 rounded-full" src=(profile_picture_url(self.user)) alt="user photo";
                    (&*self.user.username)
                    svg class="w-2.5 h-2.5 ms-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6" {
                        path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4";
                    }
                }

            div id="dropdownAvatarName" class="z-10 hidden bg-white divide-y divide-gray-100 rounded-lg shadow-sm w-44 dark:bg-gray-700 dark:divide-gray-600" {
                ul class="py-2 text-sm text-gray-700 dark:text-gray-200" aria-labelledby="dropdownInformdropdownAvatarNameButtonationButton" {
                    li {
                        a href="/agents" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-white" { "Manage agents" }
                    }
                    li {
                        a href="/settings" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-white" { "Settings" }
                    }
                }
                div class="py-2" {
                    a href="/logout" class="block px-4 py-2 text-sm text-gray-700 hover:bg-gray-100 dark:hover:bg-gray-600 dark:text-gray-200 dark:hover:text-white" { "Sign out" }
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
                path d="M4 18 C 8 18, 7 6, 12 6 C 17 6, 16 16, 20 13" stroke="#e0338a" stroke-width="2.4" fill="none" stroke-linecap="round" {}
                circle cx="20" cy="13" r="1.9" fill="#ffd84a" {}
            }
        }
    }
}

impl<'a> Render for Navbar<'a> {
    fn render(&self) -> Markup {
        let link = "text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-200 dark:hover:bg-gray-700 px-2.5 py-2 rounded text-sm font-semibold";
        html! {
            nav class="bg-white dark:bg-gray-800 border-b border-gray-300 dark:border-gray-700" {
                div class="mx-auto w-full max-w-[1280px] px-7 flex items-center gap-4 flex-wrap py-2.5" {
                    a href="/" class="flex items-center gap-2.5" {
                        (AchtungLogo)
                        span class="font-[Geologica] font-semibold text-[19px] tracking-tight text-gray-900 dark:text-white" {
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
                        @if let Some(user) = &self.session.user {
                            (UserDropdown { user });
                        }
                        @else {
                            a href="/login" class="text-sm font-semibold text-white dark:text-gray-900 bg-gray-900 dark:bg-white hover:bg-blue-700 dark:hover:bg-blue-200 px-3.5 py-2 rounded" {
                                "Sign in"
                            }
                        }
                    }
                }
            }
        }
    }
}
