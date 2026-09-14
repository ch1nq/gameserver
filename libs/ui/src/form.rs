use crate::Icon;
use crate::button::FormSubmit;
use maud::{Markup, Render, html};

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
            form class="form" method=(self.method) action=(self.action) {
                @if let Some(text) = self.helper_text {
                    (HelperText { text })
                }

                div class="form-fields" {
                    (self.fields)
                }

                // Submit button
                div class="form-actions" {
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
            div class="field" {
                label for=(self.id) class="label" {
                    (self.label) @if self.required { " *" }
                }
                input type="text" name=(self.id) id=(self.id)
                    class="input"
                    placeholder=(self.placeholder)
                    required[self.required] {}
                @if let Some(text) = self.helper_text {
                    p class="hint" { (text) }
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
            div class="field" {
                label for=(self.id) class="label" {
                    (self.label) @if self.required { " *" }
                }
                select id=(self.id) name=(self.id) required[self.required] class="select" {
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
            p class="helper" { (self.text) }
        }
    }
}
