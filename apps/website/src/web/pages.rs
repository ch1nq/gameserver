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
                            (components::form::helper_text("Deploy tokens allow you to push Docker images to the Arcadio registry. Keep your tokens secure and never share them publicly."))

                            (components::table::wrapper(
                                vec!["Name", "Created", "Actions"],
                                html! {
                                    @if tokens.is_empty() {
                                        (components::table::empty_row(3, "No tokens yet. Create your first token to start deploying agents."))
                                    } @else {
                                        @for token in tokens {
                                            (components::table::row(html! {
                                                (components::table::cell(html! { (token.name) }, true))
                                                @let format = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]");
                                                (components::table::cell(html! {
                                                    (token.created_at.format(&format).unwrap_or_else(|_| "Invalid date".to_string()))
                                                }, false))
                                                (components::table::cell(html! {
                                                    form method="post" action=(format!("/settings/tokens/{}/revoke", token.id)) onsubmit="return confirm('Are you sure you want to revoke this token? This action cannot be undone.');" {
                                                        button type="submit" class="text-red-600 hover:text-red-800 dark:text-red-400" {
                                                            "Revoke"
                                                        }
                                                    }
                                                }, false))
                                            }))
                                        }
                                    }
                                },
                                Some("mb-4"),
                            ))

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

            (components::modal::content(
                "token-created-modal",
                "Token Created Successfully",
                html! {
                    div class="p-4 md:p-5 space-y-4" {
                        (components::alert::warning("Important!", "Make sure to copy your token now. You won't be able to see it again!"))

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

                        (components::alert::info(html! {
                            p class="font-medium mb-2" { "Docker login command:" }
                            code class="text-xs" {
                                "docker login achtung-registry.fly.dev -u user-" (user_id) " -p " (plaintext_token)
                            }
                        }))
                    }
                },
                Some(html! {
                    a href="/settings" class="text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                        "Done"
                    }
                }),
                "max-w-4xl",
                true,
            ))
        },
    )
}

fn new_token_modal() -> Markup {
    components::modal::with_trigger(
        "new-token-modal",
        "Generate New Token",
        "Create Deploy Token",
        components::form::modal_form(
            "/settings/tokens/new",
            "post",
            Some(
                "Create a new deploy token for pushing Docker images. You can have up to 10 active tokens.",
            ),
            html! {
                div class="grid gap-4 mb-4 grid-cols-1" {
                    (components::form::text_input("name", "Token Name", "CI Token", Some("3-50 characters (e.g., 'CI Token', 'Local Dev')"), true))
                }
            },
            "Generate Token",
            Some(components::icon::plus()),
        ),
        None,
        components::modal::ModalSize::Small,
    )
}

pub fn agents(session: &AuthSession, agents: Vec<Agent>) -> Markup {
    components::page(
        "Agents",
        html! {
            div class="flex flex-col justify-end mt-4 gap-4" {

                h1 class="text-2xl font-semibold" { "Agents" }

                (components::table::wrapper(
                    vec!["Name", "Image", "Status", "Actions"],
                    html! {
                        @for agent in agents {
                            (components::table::row(html! {
                                (components::table::cell(html! { (agent.name.as_ref()) }, true))
                                (components::table::cell(html! {
                                    span class="text-gray-500 dark:text-gray-400 text-xs font-mono truncate max-w-xs" {
                                        (agent.image_url.as_ref())
                                    }
                                }, false))
                                (components::table::cell(html! {
                                    @let status_color = match agent.status {
                                        AgentStatus::Active => "bg-green-400",
                                        AgentStatus::Inactive => "bg-gray-400",
                                    };
                                    span class=(format!("h-3 w-3 rounded-full inline-block me-1 {}", status_color)) {}
                                    span class="text-gray-900 dark:text-white" { (format!("{:?}", agent.status)) }
                                }, false))
                                (components::table::cell(html! {
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
                                }, false))
                            }))
                        }
                    },
                    None,
                ))

                div class="flex justify-end" {
                    (new_agent_modal())
                }
            }
        },
        session,
    )
}

fn new_agent_modal() -> Markup {
    components::modal::with_trigger(
        "new-agent-modal",
        "New agent",
        "Create new agent",
        components::form::modal_form(
            "/agents/new",
            "post",
            Some(
                "Create an agent by providing a name and Docker image URL. Build and push your agent to any Docker registry (GitHub Container Registry, Docker Hub, etc.), then register it here.",
            ),
            html! {
                // Name
                div class="grid gap-4 mb-4 grid-cols-2" {
                    (components::form::text_input("name", "Name", "my-agent", Some("3-50 characters, alphanumeric with hyphens/underscores"), true))
                }
                // Docker image URL
                div class="grid gap-4 mb-4 grid-cols-2" {
                    (components::form::text_input("image_url", "Docker Image URL", "ghcr.io/username/agent:latest", Some("Full image URL including registry and tag"), true))
                }
            },
            "Add new agent",
            Some(components::icon::plus()),
        ),
        None,
        components::modal::ModalSize::Small,
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
