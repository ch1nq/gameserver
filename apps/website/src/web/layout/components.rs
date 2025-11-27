use crate::agents::agent::Agent;
use crate::users::{AuthSession, User};
use maud::{DOCTYPE, Markup, Render, html};

pub struct Base<'a> {
    pub title: &'a str,
    pub content: Markup,
}

impl<'a> Render for Base<'a> {
    fn render(&self) -> Markup {
        html! {
            (DOCTYPE)
            html {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { ("Achtung battle | ") (self.title) }
                    script src="https://unpkg.com/@tailwindcss/browser@4"{}
                    link href="https://cdn.jsdelivr.net/npm/flowbite@3.1.2/dist/flowbite.min.css" rel="stylesheet";
                }
                body class="bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-white" {
                    (self.content)
                    script src="https://cdn.jsdelivr.net/npm/flowbite@3.1.2/dist/flowbite.min.js" {};
                }
            }
        }
    }
}

pub struct Page<'a> {
    pub title: &'a str,
    pub content: Markup,
    pub session: &'a AuthSession,
}

impl<'a> Render for Page<'a> {
    fn render(&self) -> Markup {
        Base {
            title: self.title,
            content: html! {
                (Navbar { session: self.session })
                div class="container mx-10 mt-10" {
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
                            (table::Cell { content: html! { (agent.name.as_ref()) }, is_primary: true })
                        }
                    })
                }
            },
            extra_classes: Some("w-full max-w-lg"),
        }.render()
    }
}

pub mod table {
    use super::*;

    /// Creates a complete table with headers and body rows
    pub struct Table<'a> {
        pub headers: Vec<&'a str>,
        pub rows: Markup,
        pub extra_classes: Option<&'a str>,
    }

    impl<'a> Render for Table<'a> {
        fn render(&self) -> Markup {
            let wrapper_class = if let Some(extra) = self.extra_classes {
                format!("relative overflow-x-auto {}", extra)
            } else {
                "relative overflow-x-auto".to_string()
            };

            let headers = self
                .headers
                .iter()
                .map(|h| HeaderCell { text: h })
                .fold(html! {}, |acc, h| html! { (acc) (h) });

            html! {
                div class=(wrapper_class) {
                    table class="w-full text-sm text-left rtl:text-right text-gray-500 dark:text-gray-400" {
                        thead class="text-xs text-gray-700 uppercase bg-gray-50 dark:bg-gray-700 dark:text-gray-400" {
                            tr {(headers)}
                        }
                        tbody {(self.rows)}
                    }
                }
            }
        }
    }

    pub struct HeaderCell<'a> {
        pub text: &'a str,
    }

    impl<'a> Render for HeaderCell<'a> {
        fn render(&self) -> Markup {
            html! {
                th scope="col" class="px-6 py-3" { (self.text) }
            }
        }
    }

    pub struct Cell {
        pub content: Markup,
        pub is_primary: bool,
    }

    impl Render for Cell {
        fn render(&self) -> Markup {
            let class = if self.is_primary {
                "px-6 py-4 font-medium text-gray-900 whitespace-nowrap dark:text-white"
            } else {
                "px-6 py-4"
            };

            html! {
                td class=(class) { (self.content) }
            }
        }
    }

    pub struct Row {
        pub content: Markup,
    }

    impl Render for Row {
        fn render(&self) -> Markup {
            html! {
                tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                    (self.content)
                }
            }
        }
    }

    pub struct EmptyRow<'a> {
        pub colspan: usize,
        pub message: &'a str,
    }

    impl<'a> Render for EmptyRow<'a> {
        fn render(&self) -> Markup {
            html! {
                tr class="bg-white border-b dark:bg-gray-800 dark:border-gray-700 border-gray-200" {
                    td colspan=(self.colspan) class="px-6 py-4 text-center text-gray-500 dark:text-gray-400" {
                        (self.message)
                    }
                }
            }
        }
    }
}

pub mod form {
    use super::*;

    /// Creates a complete form wrapper with helper text, fields, and submit button
    pub struct ModalForm<'a> {
        pub action: &'a str,
        pub method: &'a str,
        pub helper_text: Option<&'a str>,
        pub fields: Markup,
        pub submit_text: &'a str,
        pub submit_icon: Option<Icon>,
    }

    impl<'a> Render for ModalForm<'a> {
        fn render(&self) -> Markup {
            html! {
                form class="p-4 md:p-5" method=(self.method) action=(self.action) {
                    @if let Some(text) = self.helper_text {
                        (HelperText { text })
                    }

                    div class="flex flex-col gap-4 pb-4" {
                        (self.fields)
                    }

                    // Submit button
                    div class="flex justify-end" {
                        (super::button::FormSubmit { text: self.submit_text, icon: self.submit_icon.clone() })
                    }
                }
            }
        }
    }

    pub struct TextInput<'a> {
        pub id: &'a str,
        pub label: &'a str,
        pub placeholder: &'a str,
        pub helper_text: Option<&'a str>,
        pub required: bool,
    }

    impl<'a> Render for TextInput<'a> {
        fn render(&self) -> Markup {
            html! {
                div class="col-span-2" {
                    label for=(self.id) class="block mb-2 text-sm font-medium text-gray-900 dark:text-white" {
                        (self.label) @if self.required { " *" }
                    }
                    input type="text" name=(self.id) id=(self.id)
                        class="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500"
                        placeholder=(self.placeholder)
                        required[self.required] {}
                    @if let Some(text) = self.helper_text {
                        p class="mt-1 text-xs text-gray-500 dark:text-gray-400" { (text) }
                    }
                }
            }
        }
    }

    pub struct InputOption<'a> {
        pub value: &'a str,
        pub label: &'a str,
    }

    impl<'a> InputOption<'a> {
        pub fn from_value(value: &'a str) -> Self {
            Self {
                value: value,
                label: value,
            }
        }
    }

    pub struct SelectInput<'a> {
        pub id: &'a str,
        pub label: &'a str,
        pub default_label: &'a str,
        pub options: Vec<InputOption<'a>>,
        pub required: bool,
    }

    impl<'a> Render for SelectInput<'a> {
        fn render(&self) -> Markup {
            html! {
                div class="col-span-2" {
                    label for=(self.id) class="block mb-2.5 text-sm font-medium text-heading" {
                        (self.label) @if self.required { " *" }
                    }
                    select id=(self.id) name=(self.id) required[self.required] class="block w-full px-3 py-2.5 bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-primary-600 focus:border-primary-600 block w-full p-2.5 dark:bg-gray-600 dark:border-gray-500 dark:placeholder-gray-400 dark:text-white dark:focus:ring-primary-500 dark:focus:border-primary-500" {
                        option value="" { (self.default_label) }
                        @for opt in &self.options {
                            option value=(opt.value) { (opt.label) }
                        }
                    }
                }
            }
        }
    }

    pub struct HelperText<'a> {
        pub text: &'a str,
    }

    impl<'a> Render for HelperText<'a> {
        fn render(&self) -> Markup {
            html! {
                p class="mb-4 text-sm text-gray-500 dark:text-gray-400" { (self.text) }
            }
        }
    }
}

pub mod button {
    use super::*;

    const BUTTON_CSS: &str = "text-white inline-flex items-center bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800";

    pub struct Primary<'a> {
        pub text: &'a str,
        pub url: &'a str,
        pub icon: Option<Markup>,
    }

    impl<'a> Render for Primary<'a> {
        fn render(&self) -> Markup {
            html! {
                a href=(self.url) class=(BUTTON_CSS) {
                    @if let Some(icon_markup) = &self.icon {
                        (icon_markup)
                    }
                    (self.text)
                }
            }
        }
    }

    pub struct FormSubmit<'a> {
        pub text: &'a str,
        pub icon: Option<Icon>,
    }

    impl<'a> Render for FormSubmit<'a> {
        fn render(&self) -> Markup {
            html! {
                button type="submit" class=(BUTTON_CSS) {
                    @if let Some(icon_markup) = &self.icon {
                        (icon_markup)
                    }
                    (self.text)
                }
            }
        }
    }

    pub struct ModalTrigger<'a> {
        pub modal_id: &'a str,
        pub text: &'a str,
    }

    impl<'a> Render for ModalTrigger<'a> {
        fn render(&self) -> Markup {
            html! {
                button data-modal-target=(self.modal_id) data-modal-toggle=(self.modal_id)
                    class="block text-white bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800"
                    type="button" {
                    (self.text)
                }
            }
        }
    }
}

pub mod alert {
    use super::*;

    pub struct Warning<'a> {
        pub title: &'a str,
        pub message: &'a str,
    }

    impl<'a> Render for Warning<'a> {
        fn render(&self) -> Markup {
            html! {
                div class="p-4 mb-4 text-sm text-yellow-800 rounded-lg bg-yellow-50 dark:bg-gray-800 dark:text-yellow-300" role="alert" {
                    span class="font-medium" { (self.title) }
                    " " (self.message)
                }
            }
        }
    }

    pub struct Info {
        pub content: Markup,
    }

    impl Render for Info {
        fn render(&self) -> Markup {
            html! {
                div class="p-4 text-sm text-gray-800 rounded-lg bg-gray-50 dark:bg-gray-800 dark:text-gray-300" {
                    (self.content)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Icon {
    Plus,
    Close,
    Copy,
    Checkmark,
    GithubLogo,
}

impl Render for Icon {
    fn render(&self) -> Markup {
        match self {
            Icon::Plus => html! {
                svg class="me-1 -ms-1 w-5 h-5" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" {
                    path fill-rule="evenodd" d="M10 5a1 1 0 011 1v3h3a1 1 0 110 2h-3v3a1 1 0 11-2 0v-3H6a1 1 0 110-2h3V6a1 1 0 011-1z" clip-rule="evenodd" {}
                }
            },
            Icon::Close => html! {
                svg class="w-3 h-3" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 14 14" {
                    path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 6 6m0 0 6 6M7 7l6-6M7 7l-6 6" {}
                }
            },
            Icon::Copy => html! {
                svg class="w-3.5 h-3.5" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 18 20" {
                    path d="M16 1h-3.278A1.992 1.992 0 0 0 11 0H7a1.993 1.993 0 0 0-1.722 1H2a2 2 0 0 0-2 2v15a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V3a2 2 0 0 0-2-2Zm-3 14H5a1 1 0 0 1 0-2h8a1 1 0 0 1 0 2Zm0-4H5a1 1 0 0 1 0-2h8a1 1 0 1 1 0 2Zm0-5H5a1 1 0 0 1 0-2h2V2h4v2h2a1 1 0 1 1 0 2Z" {}
                }
            },
            Icon::Checkmark => html! {
                svg class="w-3.5 h-3.5 text-green-500 dark:text-green-400" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 16 12" {
                    path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M1 5.917 5.724 10.5 15 1.5" {}
                }
            },
            Icon::GithubLogo => html! {
                svg class="w-4 h-4 me-2" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="currentColor" viewBox="0 0 20 20" {
                    path fill-rule="evenodd" d="M10 .333A9.911 9.911 0 0 0 6.866 19.65c.5.092.678-.215.678-.477 0-.237-.01-1.017-.014-1.845-2.757.6-3.338-1.169-3.338-1.169a2.627 2.627 0 0 0-1.1-1.451c-.9-.615.07-.6.07-.6a2.084 2.084 0 0 1 1.518 1.021 2.11 2.11 0 0 0 2.884.823c.044-.503.268-.973.63-1.325-2.2-.25-4.516-1.1-4.516-4.9A3.832 3.832 0 0 1 4.7 7.068a3.56 3.56 0 0 1 .095-2.623s.832-.266 2.726 1.016a9.409 9.409 0 0 1 4.962 0c1.89-1.282 2.717-1.016 2.717-1.016.366.83.402 1.768.1 2.623a3.827 3.827 0 0 1 1.02 2.659c0 3.807-2.319 4.644-4.525 4.889a2.366 2.366 0 0 1 .673 1.834c0 1.326-.012 2.394-.012 2.72 0 .263.18.572.681.475A9.911 9.911 0 0 0 10 .333Z" clip-rule="evenodd" {}
                }
            },
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
    pub struct WithTrigger<'a> {
        pub modal_id: &'a str,
        pub trigger_text: &'a str,
        pub title: &'a str,
        pub body: Markup,
        pub footer: Option<Markup>,
        pub size: ModalSize,
    }

    impl<'a> Render for WithTrigger<'a> {
        fn render(&self) -> Markup {
            html! {
                (super::button::ModalTrigger { modal_id: self.modal_id, text: self.trigger_text })
                (Content {
                    modal_id: self.modal_id,
                    title: self.title,
                    body: self.body.clone(),
                    footer: self.footer.clone(),
                    size: &self.size,
                    visible: false,
                })
            }
        }
    }

    pub struct Content<'a> {
        pub modal_id: &'a str,
        pub title: &'a str,
        pub body: Markup,
        pub footer: Option<Markup>,
        pub size: &'a ModalSize,
        pub visible: bool,
    }

    /// Creates just the modal content without trigger button
    /// Useful for modals that are shown programmatically (like success pages)
    impl<'a> Render for Content<'a> {
        fn render(&self) -> Markup {
            let max_width = match self.size {
                ModalSize::Small => "max-w-3xl",
                ModalSize::Medium => "max-w-4xl",
                ModalSize::Large => "max-w-7xl",
            };

            html! {
                div id=(self.modal_id) tabindex="-1" aria-hidden="true"
                    class="overflow-y-auto overflow-x-hidden fixed top-0 right-0 left-0 z-50 justify-center items-center w-full md:inset-0 h-[calc(100%-1rem)] max-h-full hidden" {
                    div class=(format!("relative p-4 w-full {} max-h-full", max_width)) {
                        div class="relative bg-white rounded-lg shadow-sm dark:bg-gray-700" {
                            // Modal header
                            div class="flex items-center justify-between p-4 md:p-5 border-b rounded-t dark:border-gray-600 border-gray-200" {
                                h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                    (self.title)
                                }
                                button type="button" class="text-gray-400 bg-transparent hover:bg-gray-200 hover:text-gray-900 rounded-lg text-sm w-8 h-8 ms-auto inline-flex justify-center items-center dark:hover:bg-gray-600 dark:hover:text-white" data-modal-toggle=(self.modal_id) {
                                    (super::Icon::Close)
                                    span class="sr-only" { "Close modal" }
                                }
                            }
                            // Modal body
                            (self.body)
                            // Modal footer (optional)
                            @if let Some(footer_content) = &self.footer {
                                div class="flex items-center p-4 md:p-5 border-t border-gray-200 rounded-b dark:border-gray-600" {
                                    (footer_content)
                                }
                            }
                        }
                    }
                }
                @if self.visible {
                    script {
                        "document.addEventListener('DOMContentLoaded', function() {"
                            "const modalEl = document.getElementById('" (self.modal_id) "');"
                            "const modal = new Modal(modalEl);"
                            "modal.show();"
                        "});"
                    }
                }
            }
        }
    }
}
