use maud::{Markup, Render, html};

use crate::{Icon, icon};

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
        let icon = Icon::Info; // TODO: base on severity
        html! {
            div class="p-4 mb-4 text-sm text-fg-warning rounded-base bg-warning-soft border border-warning-subtle" {
                div class="flex items-center justify-between" {
                    div class="flex items-center" {
                        (icon)
                        span class="sr-only"{ "Info" }
                        h3 class="font-medium" { (self.title) }
                    }
                }
                div class="mt-2 mb-4" { (self.message) }
            }
        }
    }
}
