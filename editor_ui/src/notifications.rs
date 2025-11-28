//! Notification/toast system for user feedback.
//!
//! Provides transient notifications for operations like save, replace, etc.

use std::time::{Duration, Instant};

/// Type of notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    /// Success notification (green).
    Success,
    /// Info notification (blue).
    Info,
    /// Warning notification (yellow).
    Warning,
    /// Error notification (red).
    Error,
}

impl NotificationType {
    /// Returns the color for this notification type.
    pub fn color(&self) -> [f32; 4] {
        match self {
            NotificationType::Success => [0.2, 0.7, 0.3, 0.95],  // Green
            NotificationType::Info => [0.25, 0.55, 0.85, 0.95],  // Blue
            NotificationType::Warning => [0.9, 0.7, 0.1, 0.95],  // Yellow/Amber
            NotificationType::Error => [0.85, 0.3, 0.25, 0.95],  // Red
        }
    }

    /// Returns the text color for this notification type.
    pub fn text_color(&self) -> [f32; 4] {
        match self {
            NotificationType::Warning => [0.1, 0.1, 0.1, 1.0],  // Dark text for yellow
            _ => [1.0, 1.0, 1.0, 1.0],  // White text
        }
    }
}

/// A single notification.
#[derive(Debug, Clone)]
pub struct Notification {
    /// The notification message.
    pub message: String,
    /// Type of notification.
    pub notification_type: NotificationType,
    /// When the notification was created.
    pub created_at: Instant,
    /// How long the notification should be visible.
    pub duration: Duration,
}

impl Notification {
    /// Creates a new notification.
    pub fn new(message: impl Into<String>, notification_type: NotificationType) -> Self {
        Self {
            message: message.into(),
            notification_type,
            created_at: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    /// Creates a new notification with custom duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Returns whether this notification has expired.
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }

    /// Returns the remaining visibility (0.0 to 1.0) for fade-out effect.
    pub fn visibility(&self) -> f32 {
        let elapsed = self.created_at.elapsed();
        if elapsed >= self.duration {
            return 0.0;
        }

        // Fade out in the last 500ms
        let fade_duration = Duration::from_millis(500);
        let remaining = self.duration - elapsed;

        if remaining < fade_duration {
            remaining.as_secs_f32() / fade_duration.as_secs_f32()
        } else {
            1.0
        }
    }
}

/// Manages notifications for the editor.
#[derive(Debug, Default)]
pub struct NotificationManager {
    /// Active notifications.
    notifications: Vec<Notification>,
    /// Maximum number of visible notifications.
    max_visible: usize,
}

impl NotificationManager {
    /// Creates a new notification manager.
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_visible: 5,
        }
    }

    /// Adds a notification.
    pub fn notify(&mut self, message: impl Into<String>, notification_type: NotificationType) {
        let notification = Notification::new(message, notification_type);
        self.notifications.push(notification);

        // Limit total notifications
        while self.notifications.len() > self.max_visible * 2 {
            self.notifications.remove(0);
        }
    }

    /// Adds a success notification.
    pub fn success(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Success);
    }

    /// Adds an info notification.
    pub fn info(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Info);
    }

    /// Adds a warning notification.
    pub fn warning(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Warning);
    }

    /// Adds an error notification.
    pub fn error(&mut self, message: impl Into<String>) {
        self.notify(message, NotificationType::Error);
    }

    /// Removes expired notifications and returns whether any are still visible.
    pub fn update(&mut self) -> bool {
        self.notifications.retain(|n| !n.is_expired());
        !self.notifications.is_empty()
    }

    /// Returns the visible notifications (most recent first).
    pub fn visible(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.iter().rev().take(self.max_visible)
    }

    /// Returns whether there are any visible notifications.
    pub fn has_notifications(&self) -> bool {
        !self.notifications.is_empty()
    }

    /// Clears all notifications.
    pub fn clear(&mut self) {
        self.notifications.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notification = Notification::new("Test message", NotificationType::Success);
        assert_eq!(notification.message, "Test message");
        assert!(!notification.is_expired());
        assert!(notification.visibility() > 0.9);
    }

    #[test]
    fn test_notification_manager() {
        let mut manager = NotificationManager::new();
        manager.success("Saved!");
        manager.error("Failed!");

        assert!(manager.has_notifications());
        assert_eq!(manager.visible().count(), 2);
    }
}
