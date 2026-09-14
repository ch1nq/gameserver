use crate::{
    alert::{Alert, AlertSeverity},
    modal::{Content, ModalSize},
};
use maud::{Markup, Render, html};

#[derive(Debug, Clone)]
pub enum ErrorType {
    Validation,
    BusinessLogic,
    System,
}

pub struct Error {
    pub title: String,
    pub message: String,
    pub error_type: ErrorType,
}

impl Error {
    pub fn internal_error(message: &str) -> Self {
        Error {
            title: "Internal error".into(),
            message: message.into(),
            error_type: ErrorType::System,
        }
    }

    pub fn validation_error(message: &str) -> Self {
        Error {
            title: "Validation error".into(),
            message: message.into(),
            error_type: ErrorType::Validation,
        }
    }

    fn icon(&self) -> Markup {
        match self.error_type {
            ErrorType::Validation | ErrorType::System => html! {
                svg class="error-svg" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" {
                    path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd" {}
                }
            },
            ErrorType::BusinessLogic => html! {
                svg class="error-svg is-warning" fill="currentColor" viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" {
                    path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd" {}
                }
            },
        }
    }

    pub fn as_content(&self) -> Markup {
        let box_class = match self.error_type {
            ErrorType::Validation | ErrorType::System => "error-box",
            ErrorType::BusinessLogic => "error-box is-business",
        };
        html! {
            div class="error-content" {
                div class="error-icon" {
                    (self.icon())
                }
                div class=(box_class) {
                    p {
                        (self.message)
                    }
                }
            }
        }
    }

    pub fn as_alert(&self) -> Markup {
        let severity = match self.error_type {
            ErrorType::Validation => AlertSeverity::Warning,
            ErrorType::BusinessLogic => AlertSeverity::Info,
            ErrorType::System => AlertSeverity::Danger,
        };
        Alert {
            title: self.title.clone(),
            message: self.message.clone(),
            severity,
        }
        .render()
    }

    pub fn as_modal(&self) -> Markup {
        let body = self.as_content();

        let footer = html! {
            a href="#" class="btn" {
                "Close"
            }
        };

        Content {
            modal_id: "error-modal",
            title: &self.title,
            body,
            footer: Some(footer),
            size: &ModalSize::Medium,
            visible: true,
        }
        .render()
    }
}

impl Render for Error {
    fn render(&self) -> Markup {
        self.as_alert()
    }
}
