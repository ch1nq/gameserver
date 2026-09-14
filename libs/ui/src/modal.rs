use crate::Icon;
use crate::button::ModalTrigger;
use maud::{Markup, Render, html};

pub enum ModalSize {
    Small,
    Medium,
    Large,
}

/// Zero-JS modal: the trigger is an anchor to `#modal_id` and the dialog
/// shows via `.modal:target`. Closing links back to `#` (or another anchor).
/// For form modals, pass a form element as the body.
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
        let dialog_class = match self.size {
            ModalSize::Small => "modal-dialog is-small",
            ModalSize::Medium => "modal-dialog",
            ModalSize::Large => "modal-dialog is-large",
        };
        let modal_class = if self.visible {
            "modal is-open"
        } else {
            "modal"
        };

        html! {
            div id=(self.modal_id) class=(modal_class) role="dialog" aria-modal="true" aria-label=(self.title) {
                div class=(dialog_class) {
                    div class="modal-card" {
                        // Modal header
                        div class="modal-head" {
                            h3 class="modal-title" {
                                (self.title)
                            }
                            a href="#" class="icon-btn" aria-label="Close modal" {
                                (Icon::Close)
                                span class="sr-only" { "Close modal" }
                            }
                        }
                        // Modal body
                        (self.body)
                        // Modal footer (optional)
                        @if let Some(footer_content) = &self.footer {
                            div class="modal-foot" {
                                (footer_content)
                            }
                        }
                    }
                }
            }
        }
    }
}
