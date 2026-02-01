use crate::Icon;
use maud::{Markup, Render, html};

const BUTTON_CSS: &str = "text-white inline-flex items-center bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-5 py-2.5 text-center dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800";

pub struct Primary<'a> {
    pub text: &'a str,
    pub url: &'a str,
    pub icon: Option<Icon>,
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
