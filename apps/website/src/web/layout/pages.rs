use crate::agents::agent::{Agent, AgentStatus};
use crate::registry::RegistryToken;
use crate::tournament_mananger::AgentImage;
use crate::users::{AuthSession, UserId};
use crate::web::layout::components;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use maud::{Markup, PreEscaped, Render, html};

pub fn home(session: &AuthSession, agents: Vec<Agent>) -> Markup {
    components::Page {
        title: "Achtung! battle",
        content: html! {
            div class="flex flex-col lg:flex-row gap-4" {
                (components::AchtungLive)
                (components::Leaderboard { agents })
            }
        },
        session,
    }
    .render()
}

pub fn login(next: Option<String>, message: Option<String>) -> Markup {
    components::Base {
        title: "Login",
        content: html! {
            div class="flex items-center justify-center h-screen" {
                div class="max-w-sm p-6 bg-white border border-gray-200 rounded-lg shadow-sm dark:bg-gray-800 dark:border-gray-700" {
                    h5 class="mb-2 text-2xl font-bold tracking-tight text-gray-900 dark:text-white" { "Login" }
                    p class="mb-3 font-normal text-gray-700 dark:text-gray-400" { "Sign in with your Github account." }

                    @if let Some(message) = message {
                        span { (message) }
                    }

                    form method="post" {
                        button type="submit" class="text-white bg-[#24292F] hover:bg-[#24292F]/90 focus:ring-4 focus:outline-none focus:ring-[#24292F]/50 font-medium rounded-lg text-sm px-5 py-2.5 text-center inline-flex items-center dark:focus:ring-gray-500 dark:hover:bg-[#050708]/30 me-2 mb-2" {
                            (components::Icon::GithubLogo)
                            "Sign in with Github"
                        }

                        @if let Some(next) = next {
                            input type="hidden" name="next" value=(next);
                        }
                    }
                }
            }
        },
    }.render()
}

pub fn settings(
    session: &AuthSession,
    tokens: Vec<RegistryToken>,
    token_created: Option<TokenCreated>,
) -> impl IntoResponse {
    let Some(user) = &session.user else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    components::Page {
        title: "Settings",
        content: html! {
            div class="flex flex-col gap-8" {
                // Profile section
                div {
                    h1 class="text-2xl font-semibold mb-4" { "Profile settings" }
                    div id="profile-picture" class="flex items-center gap-4" {
                        img class="w-16 h-16 rounded-full" src=(components::profile_picture_url(&user)) alt="user photo";
                        div {
                            p { "Username: " (user.username) }
                        }
                    }
                }

                // Deploy tokens section
                div {
                    h2 class="text-xl font-semibold mb-4" { "Deploy Tokens" }
                    (components::form::HelperText { text: "Deploy tokens allow you to push Docker images to the Arcadio registry. Keep your tokens secure and never share them publicly." })

                    (components::table::Table {
                        headers: vec!["Name", "Created", "Actions"],
                        rows: html! {
                            @if tokens.is_empty() {
                                (components::table::EmptyRow { colspan: 3, message: "No tokens yet. Create your first token to start deploying agents." })
                            } @else {
                                @for token in tokens {
                                    (components::table::Row {
                                        content: html! {
                                            (components::table::Cell { content: html! { (token.name) }, is_primary: true })
                                            @let format = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]");
                                            (components::table::Cell {
                                                content: html! {
                                                    (token.created_at.format(&format).unwrap_or_else(|_| "Invalid date".to_string()))
                                                },
                                                is_primary: false
                                            })
                                            (components::table::Cell {
                                                content: html! {
                                                    form method="post" action=(format!("/settings/tokens/{}/revoke", token.id)) onsubmit="return confirm('Are you sure you want to revoke this token? This action cannot be undone.');" {
                                                        button type="submit" class="text-red-600 hover:text-red-800 dark:text-red-400" {
                                                            "Revoke"
                                                        }
                                                    }
                                                },
                                                is_primary: false
                                            })
                                        }
                                    })
                                }
                            }
                        },
                        extra_classes: Some("mb-4"),
                    })

                    div class="flex justify-end" {
                        (new_token_modal())
                    }

                    @if let Some(token_created) = token_created {
                        (token_created.render_modal())
                    }
                }
            }
        },
        session,
    }.render().into_response()
}

pub struct TokenCreated {
    user_id: UserId,
    plaintext_token: String,
}

impl TokenCreated {
    pub fn new(user_id: UserId, plaintext_token: String) -> TokenCreated {
        TokenCreated {
            user_id,
            plaintext_token,
        }
    }

    fn render_modal(&self) -> Markup {
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

        let size = components::modal::ModalSize::Medium;
        let modal_content = components::modal::Content {
            modal_id: "token-created-modal",
            title: "Token Created Successfully",
            body: html! {
                div class="p-4 md:p-5 space-y-4" {
                    (components::alert::Warning { title: "Important!", message: "Make sure to copy your token now. You won't be able to see it again!" })

                    p class="text-base leading-relaxed text-gray-500 dark:text-gray-400" {
                        "Your deploy token has been created. Use this token to authenticate when pushing Docker images to the Arcadio registry:"
                    }

                    div class="relative" {
                        input type="text" id="token-value" readonly="" value=(&self.plaintext_token) class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block w-full p-2.5 pr-20 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white font-mono" {}
                        button onclick="copyToken()" class="absolute end-2 top-1/2 -translate-y-1/2 text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg p-2 inline-flex items-center justify-center" {
                            span id="default-icon" {
                                (components::Icon::Copy)
                            }
                            span id="success-icon" class="hidden" {
                                (components::Icon::Checkmark)
                            }
                        }
                    }

                    (components::alert::Info {
                        content: html! {
                            p class="font-medium mb-2" { "Docker login command:" }
                            code class="text-xs" {
                                "docker login achtung-registry.fly.dev -u user-" (&self.user_id) " -p " (&self.plaintext_token)
                            }
                        }
                    })
                }
            },
            footer: Some(html! {
                a href="/settings" class="text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                    "Done"
                }
            }),
            size: &size,
            visible: true,
        };

        html! {
            script {(copy_token_script)}
            (modal_content)
        }
    }
}

fn new_token_modal() -> Markup {
    components::modal::WithTrigger {
        modal_id: "new-token-modal",
        trigger_text: "Generate New Token",
        title: "Create Deploy Token",
        body: (components::form::ModalForm {
            action: "/settings/tokens/new",
            method: "post",
            helper_text: Some(
                "Create a new deploy token for pushing Docker images. You can have up to 10 active tokens.",
            ),
            fields: html! {
                div class="grid gap-4 mb-4 grid-cols-1" {
                    (components::form::TextInput {
                        id: "name",
                        label: "Token Name",
                        placeholder: "CI Token",
                        helper_text: Some("3-50 characters (e.g., 'CI Token', 'Local Dev')"),
                        required: true,
                    })
                }
            },
            submit_text: "Generate Token",
            submit_icon: Some(components::Icon::Plus),
        }).render(),
        footer: None,
        size: components::modal::ModalSize::Medium,
    }.render()
}

pub fn agents(session: &AuthSession, agents: Vec<Agent>) -> Markup {
    let rows = agents.iter().map(|agent| {
        components::table::Row {
            content: html! {
                (components::table::Cell { content: html! { (agent.name.as_ref()) }, is_primary: true })
                (components::table::Cell {
                    content: html! {
                        span class="text-gray-500 dark:text-gray-400 text-xs font-mono truncate max-w-xs" {
                            (agent.image_url.as_ref())
                        }
                    },
                    is_primary: false
                })
                (components::table::Cell {
                    content: html! {
                        @let status_color = match agent.status {
                            AgentStatus::Active => "bg-green-400",
                            AgentStatus::Inactive => "bg-gray-400",
                        };
                        span class=(format!("h-3 w-3 rounded-full inline-block me-1 {}", status_color)) {}
                        span class="text-gray-900 dark:text-white" { (format!("{:?}", agent.status)) }
                    },
                    is_primary: false
                })
                (components::table::Cell {
                    content: html! {
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
                    },
                    is_primary: false
                })
            }
        }
    });
    let table = components::table::Table {
        headers: vec!["Name", "Image", "Status", "Actions"],
        rows: html! {
            @if agents.is_empty() {
                (components::table::EmptyRow { colspan: 4, message: "No agents yet." })
            } @else { @for row in rows { (row.render()) }}
        },
        extra_classes: None,
    };

    components::Page {
        title: "Agents",
        content: html! {
            div class="flex flex-col justify-end mt-4 gap-4" {
                h1 class="text-2xl font-semibold" { "Agents" }
                (table)
                div class="flex justify-end" {
                    (components::button::Primary { text: "New agent", url: "/agents/new", icon: None })
                }
            }
        },
        session,
    }.render()
}

pub fn new_agent_page(user_images: Vec<AgentImage>, session: &AuthSession) -> Markup {
    let images = user_images
        .iter()
        .map(|img| components::form::InputOption::from_value(&img.image_url))
        .collect();
    let form = components::form::ModalForm {
        action: "/agents/new",
        method: "post",
        helper_text: Some(
            "Create an agent by providing a name and choosing an image that is pushed to the achtung registry.",
        ),
        fields: html! {
            (components::form::TextInput {
                id: "name",
                label: "Name",
                placeholder: "my-agent",
                helper_text: Some("3-50 characters, alphanumeric with hyphens/underscores"),
                required: true,
            })
            (components::form::SelectInput {
                id: "image",
                label: "Select image",
                default_label: "Choose image",
                options: images,
                required: true,
            })
        },
        submit_text: "Add new agent",
        submit_icon: Some(components::Icon::Plus),
    }.render();
    components::Page {
        title: "Create new agent",
        content: form,
        session,
    }
    .render()
}

pub fn not_found() -> Markup {
    components::Base {
        title: "Not Found",
        content: html! {
            div class="text-center mt-20" {
                h1 { "Not Found" }
                p { "The page you are looking for does not exist." }
            }
        },
    }
    .render()
}
