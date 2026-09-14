use crate::Icon;
use maud::{Markup, Render, html};

/// Accent call-to-action, driven by the mockup tokens in `base` so it
/// follows `html[data-theme]` (no `dark:` variants).
const BUTTON_CSS: &str = "inline-flex items-center font-semibold rounded-[3px] text-sm px-5 py-2.5 text-center text-[var(--accent-ink)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] focus:ring-4 focus:outline-none";

pub struct Primary<'a> {
    pub text: &'a str,
    pub url: &'a str,
    pub icon: Option<Icon>,
}

impl<'a> Render for Primary<'a> {
    fn render(&self) -> Markup {
        html! {
            a href=(self.url) class=(BUTTON_CSS) style="color:var(--accent-ink);" {
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
                class="block font-semibold rounded-[3px] text-sm px-5 py-2.5 text-center text-[var(--accent-ink)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] focus:ring-4 focus:outline-none"
                type="button" {
                (self.text)
            }
        }
    }
}
