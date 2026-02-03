use crate::users::{AuthSession, User, UserId};
use crate::web::layout::components::{self, Page};
use achtung_core::agents::agent::{Agent, AgentStatus};
use achtung_core::api_tokens::ApiToken;
use achtung_core::registry::RegistryToken;
use achtung_ui::error::Error;
use maud::{html, Markup, PreEscaped, Render};

pub fn home(session: &AuthSession, agents: Vec<Agent>) -> Page<'_> {
    Page {
        title: "Achtung! battle",
        content: html! {
            div class="flex flex-col lg:flex-row gap-4" {
                (components::AchtungLive)
                (components::Leaderboard { agents })
            }
        },
        session,
        errors: vec![],
    }
}

pub fn error_page(error: Error, session: &AuthSession) -> Page<'_> {
    Page {
        title: "An error has occurred",
        content: error.as_content(),
        session,
        errors: vec![],
    }
}

pub fn login(next: Option<String>, message: Option<String>) -> Markup {
    achtung_ui::base::Base {
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

pub fn settings<'a>(
    session: &'a AuthSession,
    user: &'a User,
    tokens: Vec<RegistryToken>,
    api_tokens: Vec<ApiToken>,
) -> Page<'a> {
    Page {
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
                (token_section(
                    "Deploy Tokens",
                    "Deploy tokens allow you to push Docker images to the Arcadio registry. Keep your tokens secure and never share them publicly.",
                    "No tokens yet. Create your first token to start deploying agents.",
                    "/settings/tokens",
                    &tokens.iter().map(|t| TokenRow { id: t.id, name: &t.name, created_at: &t.created_at }).collect::<Vec<_>>(),
                    "new-deploy-token-modal",
                    "Create Deploy Token",
                    "/settings/tokens/new",
                    "Create a new deploy token for pushing Docker images. You can have up to 10 active tokens.",
                ))

                // API tokens section
                (token_section(
                    "API Tokens",
                    "API tokens allow you to access the Achtung API from the CLI or other tools. Keep your tokens secure.",
                    "No API tokens yet. Create one to use the CLI.",
                    "/settings/api-tokens",
                    &api_tokens.iter().map(|t| TokenRow { id: t.id, name: &t.name, created_at: &t.created_at }).collect::<Vec<_>>(),
                    "new-api-token-modal",
                    "Create API Token",
                    "/settings/api-tokens/new",
                    "Create a new API token for CLI and API access. You can have up to 10 active tokens.",
                ))
            }
        },
        session,
        errors: vec![],
    }
}

pub fn token_created(
    user_id: UserId,
    plaintext_token: String,
    auth_session: &AuthSession,
) -> Markup {
    token_created_page(
        "Token created successfully!",
        "Your deploy token has been created. Use this token to authenticate when pushing Docker images to the Arcadio registry:",
        &plaintext_token,
        html! {
            p class="font-medium mb-2" { "Docker login command:" }
            code class="text-xs" {
                "docker login achtung-registry.fly.dev -u user-" (user_id) " -p " (plaintext_token)
            }
        },
        auth_session,
    )
}

pub fn api_token_created(
    user_id: UserId,
    plaintext_token: String,
    auth_session: &AuthSession,
) -> Markup {
    token_created_page(
        "API Token created successfully!",
        "Your API token has been created. Add it to your CLI config or use it as an environment variable:",
        &plaintext_token,
        html! {
            p class="font-medium mb-2" { "CLI config (~/.config/achtung/config.toml):" }
            code class="text-xs block bg-gray-100 dark:bg-gray-700 p-3 rounded-lg" {
                "api_url = \"http://localhost:3000\"\n"
                "user_id = " (user_id) "\n"
                "api_token = \"" (plaintext_token) "\""
            }
        },
        auth_session,
    )
}

fn token_created_page(
    title: &str,
    description: &str,
    plaintext_token: &str,
    usage_snippet: Markup,
    auth_session: &AuthSession,
) -> Markup {
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

    let content = html! {
        script {(copy_token_script)}
        div class="flex flex-col gap-4" {
            h1 class="mb-3 text-2xl font-semibold tracking-tight text-heading leading-8" {
                (title)
            }

            (components::alert::Alert::warning("Important!", "Make sure to copy your token now. You won't be able to see it again!"))

            p class="text-base leading-relaxed text-gray-500 dark:text-gray-400" {
                (description)
            }

            div class="relative" {
                input type="text" id="token-value" readonly="" value=(plaintext_token) class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block w-full p-2.5 pr-20 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white font-mono" {}
                button onclick="copyToken()" class="absolute end-2 top-1/2 -translate-y-1/2 text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg p-2 inline-flex items-center justify-center" {
                    span id="default-icon" {
                        (components::Icon::Copy)
                    }
                    span id="success-icon" class="hidden" {
                        (components::Icon::Checkmark)
                    }
                }
            }

            (usage_snippet)

            div class="flex justify-end" {
                a href="/settings" class="text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                    "Done"
                }
            }
        }
    };

    Page {
        title,
        content,
        session: auth_session,
        errors: vec![],
    }
    .render()
}

struct TokenRow<'a> {
    id: i64,
    name: &'a str,
    created_at: &'a time::PrimitiveDateTime,
}

fn token_section(
    title: &str,
    description: &str,
    empty_message: &str,
    revoke_base_url: &str,
    tokens: &[TokenRow],
    modal_id: &str,
    modal_title: &str,
    modal_action: &str,
    modal_helper_text: &str,
) -> Markup {
    html! {
        div {
            h2 class="text-xl font-semibold mb-4" { (title) }
            (components::form::HelperText { text: description })

            (components::table::Table {
                headers: vec!["Name", "Created", "Actions"],
                rows: html! {
                    @if tokens.is_empty() {
                        (components::table::EmptyRow { colspan: 3, message: empty_message })
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
                                            form method="post" action=(format!("{}/{}/revoke", revoke_base_url, token.id)) onsubmit="return confirm('Are you sure you want to revoke this token? This action cannot be undone.');" {
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
                (new_token_modal(modal_id, modal_title, modal_action, modal_helper_text))
            }
        }
    }
}

fn new_token_modal(modal_id: &str, title: &str, action: &str, helper_text: &str) -> Markup {
    components::modal::WithTrigger {
        modal_id,
        trigger_text: "Generate New Token",
        title,
        body: (components::form::ModalForm {
            action,
            method: "post",
            helper_text: Some(helper_text),
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
        })
        .render(),
        footer: None,
        size: components::modal::ModalSize::Medium,
    }
    .render()
}

pub fn agents(session: &AuthSession, agents: Vec<Agent>) -> Page<'_> {
    let rows = agents.iter().map(|agent| {
        components::table::Row {
            content: html! {
                (components::table::Cell { content: html! { (&*agent.name) }, is_primary: true })
                (components::table::Cell {
                    content: html! {
                        span class="text-gray-500 dark:text-gray-400 text-xs font-mono truncate max-w-xs" {
                            (&*agent.image_url)
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

    Page {
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
        errors: vec![],
    }
}

pub fn new_agent_page(user_images: Vec<String>, session: &AuthSession) -> Page<'_> {
    let images = user_images
        .iter()
        .map(|img| components::form::InputOption::from_value(img))
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
    Page {
        title: "Create new agent",
        content: form,
        session,
        errors: vec![],
    }
}

pub fn not_found() -> Markup {
    achtung_ui::base::Base {
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
