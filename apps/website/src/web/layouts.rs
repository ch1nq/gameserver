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
        let gh_logo = html! {
            svg class="w-4 h-4 me-2" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 20 20" {
                path fill-rule="evenodd" d="M10 .333A9.911 9.911 0 0 0 6.866 19.65c.5.092.678-.215.678-.477 0-.237-.01-1.017-.014-1.845-2.757.6-3.338-1.169-3.338-1.169a2.627 2.627 0 0 0-1.1-1.451c-.9-.615.07-.6.07-.6a2.084 2.084 0 0 1 1.518 1.021 2.11 2.11 0 0 0 2.884.823c.044-.503.268-.973.63-1.325-2.2-.25-4.516-1.1-4.516-4.9A3.832 3.832 0 0 1 4.7 7.068a3.56 3.56 0 0 1 .095-2.623s.832-.266 2.726 1.016a9.409 9.409 0 0 1 4.962 0c1.89-1.282 2.717-1.016 2.717-1.016.366.83.402 1.768.1 2.623a3.827 3.827 0 0 1 1.02 2.659c0 3.807-2.319 4.644-4.525 4.889a2.366 2.366 0 0 1 .673 1.834c0 1.326-.012 2.394-.012 2.72 0 .263.18.572.681.475A9.911 9.911 0 0 0 10 .333Z" clip-rule="evenodd";
            }
        };
        components::base(
            "Login",
            html! {
                div class="flex items-center justify-center h-screen" {
                    div class="max-w-sm p-6 bg-white border border-gray-200 rounded-lg shadow-sm dark:bg-gray-800 dark:border-gray-700" {
                        h5 class="mb-2 text-2xl font-bold tracking-tight text-gray-900 dark:text-white" { "Login" }
                        p class="mb-3 font-normal text-gray-700 dark:text-gray-400" { "Sign in with your Github account." }

                        @if let Some(message) = message {
                            span { (message) }
                        }

                        form method="post" {
                            button type="submit" class="text-white bg-[#24292F] hover:bg-[#24292F]/90 focus:ring-4 focus:outline-none focus:ring-[#24292F]/50 font-medium rounded-lg text-sm px-5 py-2.5 text-center inline-flex items-center dark:focus:ring-gray-500 dark:hover:bg-[#050708]/30 me-2 mb-2" {
                                (gh_logo)
                                "Sign in with Github"
                            }

                            @if let Some(next) = next {
                                input type="hidden" name="next" value=(next);
                            }
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
                div class="flex flex-col justify-end mt-4 gap-4" {

                    h1 class="text-2xl font-semibold" { "Agents" }

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
                                            @let status_color = match agent.status {
                                                crate::agents::AgentStatus::BuildFailed => "bg-red-100",
                                                crate::agents::AgentStatus::Building => "bg-yellow-100",
                                                crate::agents::AgentStatus::Created => "bg-blue-100",
                                                crate::agents::AgentStatus::Active => "bg-green-100",
                                                crate::agents::AgentStatus::Inactive => "bg-gray-100",
                                            };
                                            span class=(format!("h-3 w-3 rounded-full inline-block me-1 {}", status_color)) {}
                                            span class="text-gray-900 dark:text-white" { (format!("{:?}", agent.status)) }
                                        }
                                        td class="px-6 py-4" {
                                            a href=(format!("/agents/{}/edit", agent.name)) class="text-blue-500 hover:text-blue-700" { "Edit" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div class="flex justify-end" {
                        (new_agent_modal())
                    }
                }
            },
            session,
        )
    }

    fn new_agent_modal() -> Markup {
        html! {
            button data-modal-target="new-agent-modal" data-modal-toggle="new-agent-modal" class="block text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" type="button" {
                "New agent"
            }

            // Main modal
            div id="new-agent-modal" tabindex="-1" aria-hidden="true" class="hidden overflow-y-auto overflow-x-hidden fixed top-0 right-0 left-0 z-50 justify-center items-center w-full md:inset-0 h-[calc(100%-1rem)] max-h-full" {
                div class="relative p-4 w-full max-w-md max-h-full" {
                    // Modal content
                    div class="relative bg-white rounded-lg shadow-sm dark:bg-gray-700" {
                        // Modal header
                        div class="flex items-center justify-between p-4 md:p-5 border-b rounded-t dark:border-gray-600 border-gray-200" {
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                "Create new agent"
                            }
                            button type="button" class="text-gray-400 bg-transparent hover:bg-gray-200 hover:text-gray-900 rounded-lg text-sm w-8 h-8 ms-auto inline-flex justify-center items-center dark:hover:bg-gray-600 dark:hover:text-white" data-modal-toggle="new-agent-modal" {
                                svg class="w-3 h-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 14 14" {
                                    path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 6 6m0 0 6 6M7 7l6-6M7 7l-6 6" {}
                                }
                                span class="sr-only" { "Close modal" }
                            }
                        }
                        // Modal body
                        form class="p-4 md:p-5" method="post" action="/agents/new" {
                            p id="helper-text-explanation" class="mb-4 text-sm text-gray-500 dark:text-gray-400" {
                                "To create a new agent, you need to provide a name and the URL to the source code. The source code must be available in a public Github repository. For more information, check the "
                                a href="https://github.com/ch1nq/achtung-client-example" class="font-medium text-blue-600 hover:underline dark:text-blue-500" {"Example repository"}
                                "."
                            }

                            // Name
                            div class="grid gap-4 mb-4 grid-cols-2" {
                                div class="col-span-2" {
                                    label for="name" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                        "Name *"
                                    }
                                    input type="text" name="name" id="name" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="Agent name" required="" {}
                                }
                            }
                            // Url for source code
                            div class="grid gap-4 mb-4 grid-cols-2" {
                                div class="col-span-2" {
                                    label for="source_code_url" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                        "Source code URL *"
                                    }
                                    input type="text" name="source_code_url" id="source_code_url" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="github.com/user/repo.git" required="" {}
                                }
                            }
                            // Dockerfile path
                            div class="grid gap-4 mb-4 grid-cols-2" {
                                div class="col-span-2" {
                                    label for="dockerfile_path" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                        "Dockerfile path"
                                    }
                                    input type="text" name="dockerfile_path" id="dockerfile_path" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="./Dockerfile" {}
                                }
                            }
                            // Context sub path
                            div class="grid gap-4 mb-4 grid-cols-2" {
                                div class="col-span-2" {
                                    label for="context_sub_path" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                        "Context sub path"
                                    }
                                    input type="text" name="context_sub_path" id="context_sub_path" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="." {}
                                }
                            }

                            // Submit button
                            div class="flex justify-end" {
                                button type="submit" class="text-white inline-flex items-center bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                                    svg class="me-1 -ms-1 w-5 h-5" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" {
                                        path fill-rule="evenodd" d="M10 5a1 1 0 011 1v3h3a1 1 0 110 2h-3v3a1 1 0 11-2 0v-3H6a1 1 0 110-2h3V6a1 1 0 011-1z" clip-rule="evenodd" {}
                                    }
                                    "Add new agent"
                                }
                            }
                        }
                    }
                }
            }
        }
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
                                td class="px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white" { (agent.name) }
                            }
                        }
                    }
                }
            }
        }
    }
}
