use crate::users::{AuthSession, User};
use achtung_core::agents::agent::Agent;
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
                div class="container mx-10 mt-10" {
                    @for error in &self.errors {
                        (error)
                    }
                    div class="mx-auto" {
                        (self.content)
                    }
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
                    (self.user.username)
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

impl<'a> Render for Navbar<'a> {
    fn render(&self) -> Markup {
        let item_styles = "block py-2 px-3 text-gray-900 border-b border-gray-100 hover:bg-gray-50 md:hover:bg-transparent md:border-0 md:hover:text-blue-600 md:p-0 dark:text-white md:dark:hover:text-blue-500 dark:hover:bg-gray-700 dark:hover:text-blue-500 md:dark:hover:bg-transparent dark:border-gray-700";
        html! {
            nav class="bg-white py-2 px-10 border-b border-gray-200 dark:bg-gray-800 dark:border-gray-700" {
                div class="container flex justify-between items-center" {
                    a href="/" class="text-2xl font-semibold text-gray-900 dark:text-white" { "Achtung battle" }
                    div class="flex items-center gap-4" {
                        @if let Some(user) = &self.session.user {
                            (UserDropdown { user });
                        }
                        @else {
                            a href="/login" class=(item_styles) { "Sign in"}
                        }
                    }
                }
            }
        }
    }
}

pub struct AchtungLive;

impl Render for AchtungLive {
    fn render(&self) -> Markup {
        html! {
            div class="flex flex-col lg:flex-row gap-4" {
                div class="border rounded-lg aspect-square overflow-hidden w-full max-w-lg dark:border-gray-700" {
                    canvas id="achtung-canvas" width="1000" height="1000" class="max-h-full h-full max-w-full w-full";
                    script src="/static/achtung-observer.js" {};
                    script { "init_game('achtung-canvas');" };
                }
            }
        }
    }
}

pub struct Leaderboard {
    pub agents: Vec<Agent>,
}

impl Render for Leaderboard {
    fn render(&self) -> Markup {
        table::Table {
            headers: vec!["Name"],
            rows: html! {
                @for agent in &self.agents {
                    (table::Row {
                        content: html! {
                            (table::Cell { content: html! { (&*agent.name) }, is_primary: true })
                        }
                    })
                }
            },
            extra_classes: Some("w-full max-w-lg"),
        }
        .render()
    }
}
