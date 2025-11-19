use crate::agents::agent::Agent;
use crate::users::{AuthSession, User};
use maud::{DOCTYPE, Markup, html};

pub fn base(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { ("Achtung battle | ") (title) }
                script src="https://unpkg.com/@tailwindcss/browser@4"{}
                link href="https://cdn.jsdelivr.net/npm/flowbite@3.1.2/dist/flowbite.min.css" rel="stylesheet";
            }
            body class="bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-white" {
                (content)
                script src="https://cdn.jsdelivr.net/npm/flowbite@3.1.2/dist/flowbite.min.js" {};
            }
        }
    }
}

pub fn page(title: &str, content: Markup, session: &AuthSession) -> Markup {
    base(
        title,
        html! {
            (navbar(session))
            div class="container px-10 mt-10" {
                div class="mx-auto" {
                    (content)
                }
            }
        },
    )
}

pub fn profile_picture_url(user: &User) -> String {
    format!("https://github.com/{}.png", user.username)
}

fn user_dropdown(user: &User) -> Markup {
    html! {
        button id="dropdownAvatarNameButton" data-dropdown-toggle="dropdownAvatarName" class="flex items-center text-sm pe-1 font-medium text-gray-900 rounded-full hover:text-blue-600 dark:hover:text-blue-500 md:me-0 focus:ring-4 focus:ring-gray-100 dark:focus:ring-gray-700 dark:text-white" type="button" {
            span class="sr-only" { "Open user menu" }
                img class="w-8 h-8 me-2 rounded-full" src=(profile_picture_url(user)) alt="user photo";
                (user.username)
                svg class="w-2.5 h-2.5 ms-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6" {
                    path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4";
                }
            }

        div id="dropdownAvatarName" class="z-10 hidden bg-white divide-y divide-gray-100 rounded-lg shadow-sm w-44 dark:bg-gray-700 dark:divide-gray-600" {
            div class="px-4 py-3 text-sm text-gray-900 dark:text-white" {
                div class="font-medium" { "Pro User" }
                div class="truncate" { (user.username) }
            }
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

pub fn navbar(session: &AuthSession) -> Markup {
    let item_styles = "block py-2 px-3 text-gray-900 border-b border-gray-100 hover:bg-gray-50 md:hover:bg-transparent md:border-0 md:hover:text-blue-600 md:p-0 dark:text-white md:dark:hover:text-blue-500 dark:hover:bg-gray-700 dark:hover:text-blue-500 md:dark:hover:bg-transparent dark:border-gray-700";
    html! {
        nav class="bg-white py-2 px-10 border-b border-gray-200 dark:bg-gray-800 dark:border-gray-700" {
            div class="container flex justify-between items-center" {
                a href="/" class="text-2xl font-semibold text-gray-900 dark:text-white" { "Achtung battle" }
                div class="flex items-center gap-4" {
                    @if let Some(user) = &session.user {
                        (user_dropdown(user));
                    }
                    @else {
                        a href="/login" class=(item_styles) { "Sign in"}
                    }
                }
            }
        }
    }
}

pub fn achtung_live() -> Markup {
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

pub fn leaderboard(agents: Vec<Agent>) -> Markup {
    html! {
        div class="relative overflow-x-auto w-full max-w-lg" {
            table class="w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400" {
                thead class="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400" {
                    tr {
                        th scope="col" class="px-6 py-3" { "Name" }
                    }
                }
                tbody {
                    @for agent in agents {
                        tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                            td class="px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white" { (agent.name.as_ref()) }
                        }
                    }
                }
            }
        }
    }
}

pub mod icons {
    use super::*;

    pub fn plus_icon() -> Markup {
        html! {
            svg class="me-1 -ms-1 w-5 h-5" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" {
                path fill-rule="evenodd" d="M10 5a1 1 0 011 1v3h3a1 1 0 110 2h-3v3a1 1 0 11-2 0v-3H6a1 1 0 110-2h3V6a1 1 0 011-1z" clip-rule="evenodd" {}
            }
        }
    }

    pub fn close_icon() -> Markup {
        html! {
            svg class="w-3 h-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 14 14" {
                path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 6 6m0 0 6 6M7 7l6-6M7 7l-6 6" {}
            }
        }
    }

    pub fn copy_icon() -> Markup {
        html! {
            svg class="w-3.5 h-3.5" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 18 20" {
                path d="M16 1h-3.278A1.992 1.992 0 0 0 11 0H7a1.993 1.993 0 0 0-1.722 1H2a2 2 0 0 0-2 2v15a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V3a2 2 0 0 0-2-2Zm-3 14H5a1 1 0 0 1 0-2h8a1 1 0 0 1 0 2Zm0-4H5a1 1 0 0 1 0-2h8a1 1 0 1 1 0 2Zm0-5H5a1 1 0 0 1 0-2h2V2h4v2h2a1 1 0 1 1 0 2Z" {}
            }
        }
    }

    pub fn checkmark_icon() -> Markup {
        html! {
            svg class="w-3.5 h-3.5 text-green-500 dark:text-green-400" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 16 12" {
                path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M1 5.917 5.724 10.5 15 1.5" {}
            }
        }
    }

    pub fn github_logo() -> Markup {
        html! {
            svg class="w-4 h-4 me-2" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 20 20" {
                path fill-rule="evenodd" d="M10 .333A9.911 9.911 0 0 0 6.866 19.65c.5.092.678-.215.678-.477 0-.237-.01-1.017-.014-1.845-2.757.6-3.338-1.169-3.338-1.169a2.627 2.627 0 0 0-1.1-1.451c-.9-.615.07-.6.07-.6a2.084 2.084 0 0 1 1.518 1.021 2.11 2.11 0 0 0 2.884.823c.044-.503.268-.973.63-1.325-2.2-.25-4.516-1.1-4.516-4.9A3.832 3.832 0 0 1 4.7 7.068a3.56 3.56 0 0 1 .095-2.623s.832-.266 2.726 1.016a9.409 9.409 0 0 1 4.962 0c1.89-1.282 2.717-1.016 2.717-1.016.366.83.402 1.768.1 2.623a3.827 3.827 0 0 1 1.02 2.659c0 3.807-2.319 4.644-4.525 4.889a2.366 2.366 0 0 1 .673 1.834c0 1.326-.012 2.394-.012 2.72 0 .263.18.572.681.475A9.911 9.911 0 0 0 10 .333Z" clip-rule="evenodd" {}
            }
        }
    }
}
