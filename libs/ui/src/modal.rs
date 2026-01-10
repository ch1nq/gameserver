use maud::{Markup, Render, html};
use crate::button::ModalTrigger;
use crate::Icon;

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
            (ModalTrigger { modal_id: self.modal_id, text: self.trigger_text })
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
                                (Icon::Close)
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
