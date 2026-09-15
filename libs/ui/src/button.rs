use crate::Icon;
use maud::{Markup, Render, html};

pub struct Primary<'a> {
    pub text: &'a str,
    pub url: &'a str,
    pub icon: Option<Icon>,
}

impl<'a> Render for Primary<'a> {
    fn render(&self) -> Markup {
        html! {
            a href=(self.url) class="btn" {
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
            button type="submit" class="btn" {
                @if let Some(icon_markup) = &self.icon {
                    (icon_markup)
                }
                (self.text)
            }
        }
    }
}

/// Accent-colored text link (e.g. secondary actions next to a primary button).
/// Generic: only carries text + url, no domain knowledge.
pub struct AccentLink<'a> {
    pub text: &'a str,
    pub url: &'a str,
}

impl<'a> Render for AccentLink<'a> {
    fn render(&self) -> Markup {
        html! {
            a href=(self.url) class="link-accent" {
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
            a href=(format!("#{}", self.modal_id)) class="btn" {
                (self.text)
            }
        }
    }
}
