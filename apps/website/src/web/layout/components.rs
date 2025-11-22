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
            div class="container mx-10 mt-10" {
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
    table::wrapper(
        vec!["Name"],
        html! {
            @for agent in agents {
                (table::row(html! {
                    (table::cell(html! { (agent.name.as_ref()) }, true))
                }))
            }
        },
        Some("w-full max-w-lg"),
    )
}

pub mod table {
    use super::*;

    /// Creates a complete table with headers and body rows
    /// Pass header names and rows content
    pub fn wrapper<H: IntoIterator<Item = &'static str>>(
        headers: H,
        rows: Markup,
        extra_classes: Option<&str>,
    ) -> Markup {
        let wrapper_class = if let Some(extra) = extra_classes {
            format!("relative overflow-x-auto {}", extra)
        } else {
            "relative overflow-x-auto".to_string()
        };

        let headers = headers
            .into_iter()
            .map(table::header_cell)
            .fold(html! {}, |acc, h| html! { (acc) (h) });

        html! {
            div class=(wrapper_class) {
                table class="w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400" {
                    thead class="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400" {
                        tr {(headers)}
                    }
                    tbody {(rows)}
                }
            }
        }
    }

    pub fn header_cell(text: &str) -> Markup {
        html! {
            th scope="col" class="px-6 py-3" { (text) }
        }
    }

    pub fn cell(content: Markup, is_primary: bool) -> Markup {
        let class = if is_primary {
            "px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white"
        } else {
            "px-6 py-4"
        };

        html! {
            td class=(class) { (content) }
        }
    }

    pub fn row(content: Markup) -> Markup {
        html! {
            tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                (content)
            }
        }
    }

    pub fn empty_row(colspan: usize, message: &str) -> Markup {
        html! {
            tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                td colspan=(colspan) class="px-6 py-4 text-center text-gray-500 dark:text-gray-400" {
                    (message)
                }
            }
        }
    }
}

pub mod form {
    use super::*;

    /// Creates a complete form wrapper with helper text, fields, and submit button
    pub fn modal_form(
        action: &str,
        method: &str,
        helper_text: Option<&str>,
        fields: Markup,
        submit_text: &str,
        submit_icon: Option<Markup>,
    ) -> Markup {
        html! {
            form class="p-4 md:p-5" method=(method) action=(action) {
                @if let Some(text) = helper_text {
                    (self::helper_text(text))
                }

                (fields)

                // Submit button
                div class="flex justify-end" {
                    (super::button::primary(submit_text, submit_icon))
                }
            }
        }
    }

    pub fn text_input(
        id: &str,
        label: &str,
        placeholder: &str,
        helper_text: Option<&str>,
        required: bool,
    ) -> Markup {
        html! {
            div class="col-span-2" {
                label for=(id) class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                    (label) @if required { " *" }
                }
                input type="text" name=(id) id=(id)
                    class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500"
                    placeholder=(placeholder)
                    required[required] {}
                @if let Some(text) = helper_text {
                    p class="mt-1 text-xs text-gray-500 dark:text-gray-400" { (text) }
                }
            }
        }
    }

    pub fn helper_text(text: &str) -> Markup {
        html! {
            p class="mb-4 text-sm text-gray-500 dark:text-gray-400" { (text) }
        }
    }
}

pub mod button {
    use super::*;

    pub fn primary(text: &str, icon: Option<Markup>) -> Markup {
        html! {
            button type="submit" class="text-white inline-flex items-center bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800" {
                @if let Some(icon_markup) = icon {
                    (icon_markup)
                }
                (text)
            }
        }
    }

    pub fn modal_trigger(modal_id: &str, text: &str) -> Markup {
        html! {
            button data-modal-target=(modal_id) data-modal-toggle=(modal_id)
                class="block text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800"
                type="button" {
                (text)
            }
        }
    }
}

pub mod alert {
    use super::*;

    pub fn warning(title: &str, message: &str) -> Markup {
        html! {
            div class="p-4 mb-4 text-sm text-yellow-800 rounded-lg bg-yellow-50 dark:bg-gray-800 dark:text-yellow-300" role="alert" {
                span class="font-medium" { (title) }
                " " (message)
            }
        }
    }

    pub fn info(content: Markup) -> Markup {
        html! {
            div class="p-4 text-sm text-gray-800 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-300" {
                (content)
            }
        }
    }
}

pub mod icon {
    use super::*;

    pub fn plus() -> Markup {
        html! {
            svg class="me-1 -ms-1 w-5 h-5" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" {
                path fill-rule="evenodd" d="M10 5a1 1 0 011 1v3h3a1 1 0 110 2h-3v3a1 1 0 11-2 0v-3H6a1 1 0 110-2h3V6a1 1 0 011-1z" clip-rule="evenodd" {}
            }
        }
    }

    pub fn close() -> Markup {
        html! {
            svg class="w-3 h-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 14 14" {
                path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 6 6m0 0 6 6M7 7l6-6M7 7l-6 6" {}
            }
        }
    }

    pub fn copy() -> Markup {
        html! {
            svg class="w-3.5 h-3.5" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 18 20" {
                path d="M16 1h-3.278A1.992 1.992 0 0 0 11 0H7a1.993 1.993 0 0 0-1.722 1H2a2 2 0 0 0-2 2v15a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V3a2 2 0 0 0-2-2Zm-3 14H5a1 1 0 0 1 0-2h8a1 1 0 0 1 0 2Zm0-4H5a1 1 0 0 1 0-2h8a1 1 0 1 1 0 2Zm0-5H5a1 1 0 0 1 0-2h2V2h4v2h2a1 1 0 1 1 0 2Z" {}
            }
        }
    }

    pub fn checkmark() -> Markup {
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

pub mod modal {
    use super::*;

    pub enum ModalSize {
        Small,
        Medium,
        Large,
    }

    /// Creates a complete modal with trigger button
    /// For form modals, pass a form element as the body
    pub fn with_trigger(
        modal_id: &str,
        trigger_text: &str,
        title: &str,
        body: Markup,
        footer: Option<Markup>,
        size: ModalSize,
    ) -> Markup {
        let modal_content = ModalContent {
            modal_id,
            title,
            body,
            footer,
            size,
            visible: false,
        };
        html! {
            (super::button::modal_trigger(modal_id, trigger_text))
            (content(&modal_content))
        }
    }

    pub struct ModalContent<'a> {
        pub modal_id: &'a str,
        pub title: &'a str,
        pub body: Markup,
        pub footer: Option<Markup>,
        pub size: ModalSize,
        pub visible: bool,
    }

    /// Creates just the modal content without trigger button
    /// Useful for modals that are shown programmatically (like success pages)
    pub fn content(modal_content: &ModalContent) -> Markup {
        let max_width = match modal_content.size {
            ModalSize::Small => "max-w-3xl",
            ModalSize::Medium => "max-w-4xl",
            ModalSize::Large => "max-w-7xl",
        };

        html! {
            div id=(&modal_content.modal_id) tabindex="-1" aria-hidden="true"
                class="overflow-y-auto overflow-x-hidden fixed top-0 right-0 left-0 z-50 justify-center items-center w-full md:inset-0 h-[calc(100%-1rem)] max-h-full hidden" {
                div class=(format!("relative p-4 w-full {} max-h-full", max_width)) {
                    div class="relative bg-white rounded-lg shadow-sm dark:bg-gray-700" {
                        // Modal header
                        div class="flex items-center justify-between p-4 md:p-5 border-b rounded-t dark:border-gray-600 border-gray-200" {
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                (&modal_content.title)
                            }
                            button type="button" class="text-gray-400 bg-transparent hover:bg-gray-200 hover:text-gray-900 rounded-lg text-sm w-8 h-8 ms-auto inline-flex justify-center items-center dark:hover:bg-gray-600 dark:hover:text-white" data-modal-toggle=(&modal_content.modal_id) {
                                (super::icon::close())
                                span class="sr-only" { "Close modal" }
                            }
                        }
                        // Modal body
                        (&modal_content.body)
                        // Modal footer (optional)
                        @if let Some(footer_content) = &modal_content.footer {
                            div class="flex items-center p-4 md:p-5 border-t border-gray-200 rounded-b dark:border-gray-600" {
                                (footer_content)
                            }
                        }
                    }
                }
            }
            @if modal_content.visible {
                script {
                    "document.addEventListener('DOMContentLoaded', function() {"
                        "const modalEl = document.getElementById('" (&modal_content.modal_id) "');"
                        "const modal = new Modal(modalEl);"
                        "modal.show();"
                    "});"
                }
            }
        }
    }
}
