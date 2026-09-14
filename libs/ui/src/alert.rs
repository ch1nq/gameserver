use maud::{Markup, Render, html};

use crate::Icon;

#[derive(Debug)]
pub enum AlertSeverity {
    Default,
    Info,
    Warning,
    Danger,
    Success,
}

pub struct Alert {
    pub title: String,
    pub message: String,
    pub severity: AlertSeverity,
}

impl Alert {
    pub fn default(title: &str, message: &str) -> Self {
        Alert {
            title: title.into(),
            message: message.into(),
            severity: AlertSeverity::Default,
        }
    }
    pub fn info(title: &str, message: &str) -> Self {
        Alert {
            title: title.into(),
            message: message.into(),
            severity: AlertSeverity::Info,
        }
    }
    pub fn warning(title: &str, message: &str) -> Self {
        Alert {
            title: title.into(),
            message: message.into(),
            severity: AlertSeverity::Warning,
        }
    }
    pub fn danger(title: &str, message: &str) -> Self {
        Alert {
            title: title.into(),
            message: message.into(),
            severity: AlertSeverity::Danger,
        }
    }
    pub fn success(title: &str, message: &str) -> Self {
        Alert {
            title: title.into(),
            message: message.into(),
            severity: AlertSeverity::Success,
        }
    }
}

impl Render for Alert {
    fn render(&self) -> Markup {
        let class = match self.severity {
            AlertSeverity::Default => "alert",
            AlertSeverity::Info => "alert alert-info",
            AlertSeverity::Warning => "alert alert-warning",
            AlertSeverity::Danger => "alert alert-danger",
            AlertSeverity::Success => "alert alert-success",
        };
        html! {
            div class=(class) role="alert" {
                div class="alert-head" {
                    div class="alert-title" {
                        (Icon::Info)
                        span class="sr-only"{ "Info" }
                        h3 { (self.title) }
                    }
                }
                div class="alert-body" { (self.message) }
            }
        }
    }
}
