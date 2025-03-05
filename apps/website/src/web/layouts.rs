use crate::agents::Agent;
use crate::users::{AuthSession, User};
use axum::http::StatusCode;
use maud::{html, Markup, DOCTYPE};

pub mod pages {
    use axum::response::IntoResponse;

    use super::*;

    pub fn home(session: &AuthSession, agents: Vec<Agent>) -> Markup {
        components::page(
            "Achtung! battle",
            html! {
                div class="flex flex-col lg:flex-row gap-4" {
                    (components::achtung_live())
                    (components::leaderboard(agents))
                }
            },
            session,
        )
    }

    pub fn login(next: Option<String>, message: Option<String>) -> Markup {
        components::base(
            "Login",
            html! {
                div class="text-center bg-white p-2 pb-10 rounded shadow-lg w-1/4 mx-auto mt-20" {
                    h1 class="text-2xl font-semibold mt-4" { "Login" }
                    p class="mt-4" { "Please login to continue" }

                    @if let Some(message) = message {
                        span { (message) }
                    }

                    form method="post" {
                        input type="submit" value="Login with GitHub" class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded mt-4";

                        @if let Some(next) = next {
                            input type="hidden" name="next" value=(next);
                        }
                    }
                }
            },
        )
    }

    pub fn settings(session: &AuthSession) -> impl IntoResponse {
        match &session.user {
            Some(user) => components::page(
                "Settings",
                html! {
                    div {
                        h1 { "Profile settings" }
                        div id="profile-picture" class="flex items-center gap-4" {
                            img class="w-16 h-16 rounded-full" src=(components::profile_picture_url(user)) alt="user photo";
                            div {
                                p { "Username: " (user.username) }
                            }
                        }
                    }
                },
                session,
            ).into_response(),
            None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }

    pub fn agents(session: &AuthSession, agents: Vec<Agent>) -> Markup {
        components::page(
            "Agents",
            html! {
                h1 class="text-2xl font-semibold mt-4" { "Agents" }

                div class="relative overflow-x-auto" {
                    table class="w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400" {
                        thead class="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400" {
                            tr {
                                th scope="col" class="px-6 py-3" { "Name" }
                                th scope="col" class="px-6 py-3" { "Status" }
                                th scope="col" class="px-6 py-3" { "Actions" }
                            }
                        }
                        tbody {
                            @for agent in agents {
                                tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                                    td class="px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white" { (agent.name) }
                                    td class="px-6 py-4" {
                                        (format!("{:?}", agent.status))
                                    }
                                    td class="px-6 py-4" {
                                        a href=(format!("/agents/{}/edit", agent.name)) class="text-blue-500 hover:text-blue-700" { "Edit" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            session,
        )
    }

    pub fn new_agent(session: &AuthSession) -> Markup {
        components::page(
            "New Agent",
            html! {
                h1 class="text-2xl font-semibold mt-4" { "New Agent" }

                form method="post" {
                    div class="mt-4" {
                        label for="name" { "Name" }
                        input type="text" name="name" id="name" class="block w-full mt-1" required;
                    }
                    div class="mt-4" {
                        label for="status" { "Status" }
                        select name="status" id="status" class="block w-full mt-1" required {
                            option value="Active" { "Active" }
                            option value="Inactive" { "Inactive" }
                        }
                    }
                    div class="mt-4" {
                        input type="submit" value="Create" class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded mt-4";
                    }
                }
            },
            session,
        )
    }

    pub fn not_found() -> Markup {
        components::base(
            "Not Found",
            html! {
                div class="text-center mt-20" {
                    h1 { "Not Found" }
                    p { "The page you are looking for does not exist." }
                }
            },
        )
    }
}

pub mod components {
    use super::*;

    pub fn base(title: &str, content: Markup) -> Markup {
        html! {
            (DOCTYPE)
            html {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { ("Achtung battle | ") (title) }
                    link href="https://cdn.jsdelivr.net/npm/tailwindcss@2.2.19/dist/tailwind.min.css" rel="stylesheet";
                    link href="https://cdn.jsdelivr.net/npm/flowbite@3.1.2/dist/flowbite.min.css" rel="stylesheet";
                }
                body class="bg-gray-100" {
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
            nav class="bg-white py-2 px-10 border-b border-gray-200" {
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
                div class="border rounded-lg aspect-square overflow-hidden w-full max-w-lg" {
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
                            th scope="col" class="px-6 py-3" { "Rank" }
                            th scope="col" class="px-6 py-3" { "Wins" }
                            th scope="col" class="px-6 py-3" { "Losses" }
                        }
                    }
                    tbody {
                        @for agent in agents {
                            tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                                td class="px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white" { (agent.name) }
                                td class="px-6 py-4" {(agent.stats.rank)}
                                td class="px-6 py-4" {(agent.stats.wins)}
                                td class="px-6 py-4" {(agent.stats.losses)}
                            }
                        }
                    }
                }
            }
        }
    }
}
