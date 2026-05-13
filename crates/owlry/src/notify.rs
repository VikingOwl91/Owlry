//! Desktop notification system
//!
//! Provides system notifications for owlry and its plugins.
//! Uses the freedesktop notification specification via notify-rust.
//!
//! Note: Some convenience functions are provided for future use and
//! are currently unused by the core (plugins use the Host API instead).

#![allow(dead_code)]

use notify_rust::{Notification, Urgency};

/// Notification urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotifyUrgency {
    /// Low priority notification
    Low,
    /// Normal priority notification (default)
    #[default]
    Normal,
    /// Critical/urgent notification
    Critical,
}

impl From<NotifyUrgency> for Urgency {
    fn from(urgency: NotifyUrgency) -> Self {
        match urgency {
            NotifyUrgency::Low => Urgency::Low,
            NotifyUrgency::Normal => Urgency::Normal,
            NotifyUrgency::Critical => Urgency::Critical,
        }
    }
}

/// Send a simple notification
pub fn notify(summary: &str, body: &str) {
    notify_with_options(summary, body, None, NotifyUrgency::Normal);
}

/// Send a notification with an icon
pub fn notify_with_icon(summary: &str, body: &str, icon: &str) {
    notify_with_options(summary, body, Some(icon), NotifyUrgency::Normal);
}

/// Send a notification with full options
pub fn notify_with_options(summary: &str, body: &str, icon: Option<&str>, urgency: NotifyUrgency) {
    let mut notification = Notification::new();
    notification
        .appname("Owlry")
        .summary(summary)
        .body(body)
        .urgency(urgency.into());

    if let Some(icon_name) = icon {
        notification.icon(icon_name);
    }

    if let Err(e) = notification.show() {
        log::warn!("Failed to show notification: {}", e);
    }
}

/// Send a notification with a timeout
pub fn notify_with_timeout(summary: &str, body: &str, icon: Option<&str>, timeout_ms: i32) {
    let mut notification = Notification::new();
    notification
        .appname("Owlry")
        .summary(summary)
        .body(body)
        .timeout(timeout_ms);

    if let Some(icon_name) = icon {
        notification.icon(icon_name);
    }

    if let Err(e) = notification.show() {
        log::warn!("Failed to show notification: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urgency_conversion() {
        assert_eq!(Urgency::from(NotifyUrgency::Low), Urgency::Low);
        assert_eq!(Urgency::from(NotifyUrgency::Normal), Urgency::Normal);
        assert_eq!(Urgency::from(NotifyUrgency::Critical), Urgency::Critical);
    }
}
