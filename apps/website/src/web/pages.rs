use crate::agents::agent::{Agent, AgentStatus};
use crate::tokens::RegistryToken;
use crate::users::AuthSession;
use axum::http::StatusCode;
use maud::{Markup, PreEscaped, html};

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
            div class="flex items-center justify-center h-screen" {
                div class="max-w-sm p-6 bg-white border border-gray-200 rounded-lg shadow-sm dark:bg-gray-800 dark:border-gray-700" {
                    h5 class="mb-2 text-2xl font-bold tracking-tight text-gray-900 dark:text-white" { "Login" }
                    p class="mb-3 font-normal text-gray-700 dark:text-gray-400" { "Sign in with your Github account." }

                    @if let Some(message) = message {
                        span { (message) }
                    }

                    form method="post" {
                        button type="submit" class="text-white bg-[#24292F] hover:bg-[#24292F]/90 focus:ring-4 focus:outline-none focus:ring-[#24292F]/50 font-medium rounded-lg text-sm px-5 py-2.5 text-center inline-flex items-center dark:focus:ring-gray-500 dark:hover:bg-[#050708]/30 me-2 mb-2" {
                            (components::icon::github_logo())
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

pub fn settings(session: &AuthSession, tokens: Vec<RegistryToken>) -> impl IntoResponse + use<> {
    match &session.user {
            Some(user) => components::page(
                "Settings",
                html! {
                    div class="flex flex-col gap-8" {
                        // Profile section
                        div {
                            h1 class="text-2xl font-semibold mb-4" { "Profile settings" }
                            div id="profile-picture" class="flex items-center gap-4" {
                                img class="w-16 h-16 rounded-full" src=(components::profile_picture_url(user)) alt="user photo";
                                div {
                                    p { "Username: " (user.username) }
                                }
                            }
                        }

                        // Deploy tokens section
                        div {
                            h2 class="text-xl font-semibold mb-4" { "Deploy Tokens" }
                            p class="mb-4 text-sm text-gray-500 dark:text-gray-400" {
                                "Deploy tokens allow you to push Docker images to the Arcadio registry. Keep your tokens secure and never share them publicly."
                            }

                            div class="relative overflow-x-auto mb-4" {
                                table class="w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400" {
                                    thead class="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400" {
                                        tr {
                                            th scope="col" class="px-6 py-3" { "Name" }
                                            th scope="col" class="px-6 py-3" { "Created" }
                                            th scope="col" class="px-6 py-3" { "Actions" }
                                        }
                                    }
                                    tbody {
                                        @if tokens.is_empty() {
                                            tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                                                td colspan="3" class="px-6 py-4 text-center text-gray-500 dark:text-gray-400" {
                                                    "No tokens yet. Create your first token to start deploying agents."
                                                }
                                            }
                                        } @else {
                                            @for token in tokens {
                                                tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                                                    td class="px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white" {
                                                        (token.name)
                                                    }
                                                    td class="px-6 py-4" {
                                                        @let format = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]");
                                                        (token.created_at.format(&format).unwrap_or_else(|_| "Invalid date".to_string()))
                                                    }
                                                    td class="px-6 py-4" {
                                                        form method="post" action=(format!("/settings/tokens/{}/revoke", token.id)) onsubmit="return confirm('Are you sure you want to revoke this token? This action cannot be undone.');" {
                                                            button type="submit" class="text-red-600 hover:text-red-800 dark:text-red-400" {
                                                                "Revoke"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div class="flex justify-end" {
                                (new_token_modal())
                            }
                        }
                    }
                },
                session,
            ).into_response(),
            None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
}

pub fn token_created(_token_id: i64, user_id: i64, plaintext_token: &str) -> Markup {
    let copy_token_script = PreEscaped(
        r#"
                async function copyToken() {
                    const tokenInput = document.getElementById('token-value');
                    const defaultIcon = document.getElementById('default-icon');
                    const successIcon = document.getElementById('success-icon');
                    try {
                        await navigator.clipboard.writeText(tokenInput.value);
                        defaultIcon.classList.add('hidden');
                        successIcon.classList.remove('hidden');
                        setTimeout(() => {
                            defaultIcon.classList.remove('hidden');
                            successIcon.classList.add('hidden');
                        }, 2000);
                    } catch (err) {
                        console.error('Failed to copy:', err);
                        alert('Failed to copy token. Please copy manually.');
                    }
                }
            "#,
    );

    components::base(
        "Token Created",
        html! {
            script {(copy_token_script)}

            // Modal backdrop
            div id="token-created-modal" tabindex="-1" aria-hidden="false" class="overflow-y-auto overflow-x-hidden fixed top-0 right-0 left-0 z-50 justify-center items-center w-full md:inset-0 h-[calc(100%-1rem)] max-h-full flex bg-gray-900 bg-opacity-50" {
                div class="relative p-4 w-full max-w-4xl max-h-full" {
                    div class="relative bg-white rounded-lg shadow-sm dark:bg-gray-700" {
                        // Modal header
                        div class="flex items-center justify-between p-4 md:p-5 border-b rounded-t dark:border-gray-600 border-gray-200" {
                            h3 class="text-xl font-semibold text-gray-900 dark:text-white" {
                                "Token Created Successfully"
                            }
                        }
                        // Modal body
                        div class="p-4 md:p-5 space-y-4" {
                            div class="p-4 mb-4 text-sm text-yellow-800 rounded-lg bg-yellow-50 dark:bg-gray-800 dark:text-yellow-300" role="alert" {
                                span class="font-medium" { "Important!" }
                                " Make sure to copy your token now. You won't be able to see it again!"
                            }

                            p class="text-base leading-relaxed text-gray-500 dark:text-gray-400" {
                                "Your deploy token has been created. Use this token to authenticate when pushing Docker images to the Arcadio registry:"
                            }

                            div class="relative" {
                                input type="text" id="token-value" readonly="" value=(plaintext_token) class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block w-full p-2.5 pr-20 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white font-mono" {}
                                button onclick="copyToken()" class="absolute end-2 top-1/2 -translate-y-1/2 text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg p-2 inline-flex items-center justify-center" {
                                    span id="default-icon" {
                                        (components::icon::copy())
                                    }
                                    span id="success-icon" class="hidden" {
                                        (components::icon::checkmark())
                                    }
                                }
                            }

                            div class="p-4 text-sm text-gray-800 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-300" {
                                p class="font-medium mb-2" { "Docker login command:" }
                                code class="text-xs" {
                                    "docker login achtung-registry.fly.dev -u user-" (user_id) " -p " (plaintext_token)
                                }
                            }
                        }
                        // Modal footer
                        div class="flex items-center p-4 md:p-5 border-t border-gray-200 rounded-b dark:border-gray-600" {
                            a href="/settings" class="text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                                "Done"
                            }
                        }
                    }
                }
            }
        },
    )
}

fn new_token_modal() -> Markup {
    html! {
        button data-modal-target="new-token-modal" data-modal-toggle="new-token-modal" class="block text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" type="button" {
            "Generate New Token"
        }

        // Main modal
        div id="new-token-modal" tabindex="-1" aria-hidden="true" class="hidden overflow-y-auto overflow-x-hidden fixed top-0 right-0 left-0 z-50 justify-center items-center w-full md:inset-0 h-[calc(100%-1rem)] max-h-full" {
            div class="relative p-4 w-full max-w-md max-h-full" {
                // Modal content
                div class="relative bg-white rounded-lg shadow-sm dark:bg-gray-700" {
                    // Modal header
                    div class="flex items-center justify-between p-4 md:p-5 border-b rounded-t dark:border-gray-600 border-gray-200" {
                        h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                            "Create Deploy Token"
                        }
                        button type="button" class="text-gray-400 bg-transparent hover:bg-gray-200 hover:text-gray-900 rounded-lg text-sm w-8 h-8 ms-auto inline-flex justify-center items-center dark:hover:bg-gray-600 dark:hover:text-white" data-modal-toggle="new-token-modal" {
                            (components::icon::close())
                            span class="sr-only" { "Close modal" }
                        }
                    }
                    // Modal body
                    form class="p-4 md:p-5" method="post" action="/settings/tokens/new" {
                        p class="mb-4 text-sm text-gray-500 dark:text-gray-400" {
                            "Create a new deploy token for pushing Docker images. You can have up to 10 active tokens."
                        }

                        div class="grid gap-4 mb-4 grid-cols-1" {
                            div {
                                label for="name" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                    "Token Name *"
                                }
                                input type="text" name="name" id="name" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="CI Token" required="" {}
                                p class="mt-1 text-xs text-gray-500 dark:text-gray-400" {
                                    "3-50 characters (e.g., 'CI Token', 'Local Dev')"
                                }
                            }
                        }

                        // Submit button
                        div class="flex justify-end" {
                            button type="submit" class="text-white inline-flex items-center bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                                (components::icon::plus())
                                "Generate Token"
                            }
                        }
                    }
                }
            }
        }
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
                                th scope="col" class="px-6 py-3" { "Image" }
                                th scope="col" class="px-6 py-3" { "Status" }
                                th scope="col" class="px-6 py-3" { "Actions" }
                            }
                        }
                        tbody {
                            @for agent in agents {
                                tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                                    td class="px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white" { (agent.name.as_ref()) }
                                    td class="px-6 py-4 text-gray-500 dark:text-gray-400 text-xs font-mono truncate max-w-xs" {
                                        (agent.image_url.as_ref())
                                    }
                                    td class="px-6 py-4" {
                                        @let status_color = match agent.status {
                                            AgentStatus::Active => "bg-green-400",
                                            AgentStatus::Inactive => "bg-gray-400",
                                        };
                                        span class=(format!("h-3 w-3 rounded-full inline-block me-1 {}", status_color)) {}
                                        span class="text-gray-900 dark:text-white" { (format!("{:?}", agent.status)) }
                                    }
                                    td class="px-6 py-4" {
                                        div class="flex gap-2" {
                                            @match agent.status {
                                                AgentStatus::Active => {
                                                    form method="post" action=(format!("/agents/{}/deactivate", agent.id)) {
                                                        button type="submit" class="text-yellow-600 hover:text-yellow-800 dark:text-yellow-400" { "Deactivate" }
                                                    }
                                                }
                                                AgentStatus::Inactive => {
                                                    form method="post" action=(format!("/agents/{}/activate", agent.id)) {
                                                        button type="submit" class="text-green-600 hover:text-green-800 dark:text-green-400" { "Activate" }
                                                    }
                                                }
                                            }
                                            form method="post" action=(format!("/agents/{}/delete", agent.id)) onsubmit="return confirm('Are you sure you want to delete this agent?');" {
                                                button type="submit" class="text-red-600 hover:text-red-800 dark:text-red-400" { "Delete" }
                                            }
                                        }
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
                            (components::icon::close())
                            span class="sr-only" { "Close modal" }
                        }
                    }
                    // Modal body
                    form class="p-4 md:p-5" method="post" action="/agents/new" {
                        p id="helper-text-explanation" class="mb-4 text-sm text-gray-500 dark:text-gray-400" {
                            "Create an agent by providing a name and Docker image URL. Build and push your agent to any Docker registry (GitHub Container Registry, Docker Hub, etc.), then register it here."
                        }

                        // Name
                        div class="grid gap-4 mb-4 grid-cols-2" {
                            div class="col-span-2" {
                                label for="name" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                    "Name *"
                                }
                                input type="text" name="name" id="name" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="my-agent" required="" {}
                                p class="mt-1 text-xs text-gray-500 dark:text-gray-400" { "3-50 characters, alphanumeric with hyphens/underscores" }
                            }
                        }
                        // Docker image URL
                        div class="grid gap-4 mb-4 grid-cols-2" {
                            div class="col-span-2" {
                                label for="image_url" class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                                    "Docker Image URL *"
                                }
                                input type="text" name="image_url" id="image_url" class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" placeholder="ghcr.io/username/agent:latest" required="" {}
                                p class="mt-1 text-xs text-gray-500 dark:text-gray-400" { "Full image URL including registry and tag" }
                            }
                        }

                        // Submit button
                        div class="flex justify-end" {
                            button type="submit" class="text-white inline-flex items-center bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                                (components::icon::plus())
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
