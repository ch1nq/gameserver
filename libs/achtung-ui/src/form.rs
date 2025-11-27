use maud::{Markup, Render, html};
use crate::button::FormSubmit;
use crate::Icon;

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
                    (FormSubmit { text: self.submit_text, icon: self.submit_icon.clone() })
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
