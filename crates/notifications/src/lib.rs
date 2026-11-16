//! Centralized notification center domain types, filtering, and sleep buffering.
//!
//! Per `TZ.md` §9, events received while the host display is off or the system
//! is transitioning/disconnected are retained in a bounded queue for delivery
//! once connectivity resumes.

use std::collections::VecDeque;

/// Priority classification for notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotificationPriority {
    /// Informational low-urgency event.
    Low,
    /// Standard application or system notification.
    Normal,
    /// Actionable warning (e.g., high disk utilization).
    High,
    /// Immediate attention required (e.g., thermal threshold breach).
    Critical,
}

/// Origin category for a notification event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationSource {
    /// Native Windows 11 system event.
    WindowsSystem,
    /// External application integrating via Universal API.
    Application(String),
    /// Internal hardware monitor alert (overheating, disk full, etc.).
    HardwareAlert,
    /// User script or WASM plugin event.
    Plugin(String),
}

/// Structured notification payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEvent {
    /// Unique event identifier.
    pub id: u64,
    /// Origin of the event.
    pub source: NotificationSource,
    /// Short heading.
    pub title: String,
    /// Detailed notification body.
    pub body: String,
    /// Urgency level.
    pub priority: NotificationPriority,
    /// Creation timestamp in Unix epoch milliseconds.
    pub created_at_epoch_ms: u64,
}

/// Filter criteria evaluated before surfacing notifications to clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationFilter {
    /// Minimum priority required to pass the filter.
    pub min_priority: NotificationPriority,
    /// Optional muted application or plugin names.
    pub muted_origins: Vec<String>,
}

impl Default for NotificationFilter {
    fn default() -> Self {
        Self {
            min_priority: NotificationPriority::Low,
            muted_origins: Vec::new(),
        }
    }
}

impl NotificationFilter {
    /// Returns `true` if `event` satisfies the filter criteria.
    #[must_use]
    pub fn matches(&self, event: &NotificationEvent) -> bool {
        if event.priority < self.min_priority {
            return false;
        }
        match &event.source {
            NotificationSource::Application(name) | NotificationSource::Plugin(name) => {
                !self.muted_origins.iter().any(|muted| muted == name)
            }
            NotificationSource::WindowsSystem | NotificationSource::HardwareAlert => true,
        }
    }
}

/// Bounded notification queue preserving recent events across sleep/disconnect cycles.
#[derive(Debug, Clone)]
pub struct NotificationQueue {
    capacity: usize,
    items: VecDeque<NotificationEvent>,
}

impl NotificationQueue {
    /// Creates a bounded notification queue with at least capacity `1`.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            items: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Enqueues an event, evicting the oldest entry if the queue is full.
    pub fn push(&mut self, event: NotificationEvent) {
        if self.items.len() == self.capacity {
            let _ = self.items.pop_front();
        }
        self.items.push_back(event);
    }

    /// Drains all queued notifications in FIFO order (e.g., on wake/reconnect).
    pub fn drain_all(&mut self) -> Vec<NotificationEvent> {
        self.items.drain(..).collect()
    }

    /// Returns the number of currently queued events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` when no events are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event(id: u64, priority: NotificationPriority, app: &str) -> NotificationEvent {
        NotificationEvent {
            id,
            source: NotificationSource::Application(app.to_owned()),
            title: format!("Event {id}"),
            body: "Details".to_owned(),
            priority,
            created_at_epoch_ms: 1_700_000_000_000 + id,
        }
    }

    #[test]
    fn bounded_queue_evicts_oldest_event() {
        let mut queue = NotificationQueue::new(2);
        queue.push(sample_event(1, NotificationPriority::Normal, "AppA"));
        queue.push(sample_event(2, NotificationPriority::High, "AppA"));
        queue.push(sample_event(3, NotificationPriority::Critical, "AppB"));

        let drained = queue.drain_all();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, 2);
        assert_eq!(drained[1].id, 3);
        assert!(queue.is_empty());
    }

    #[test]
    fn filter_respects_priority_and_muted_origins() {
        let filter = NotificationFilter {
            min_priority: NotificationPriority::High,
            muted_origins: vec!["NoisyApp".to_owned()],
        };

        assert!(!filter.matches(&sample_event(1, NotificationPriority::Normal, "GoodApp")));
        assert!(!filter.matches(&sample_event(2, NotificationPriority::Critical, "NoisyApp")));
        assert!(filter.matches(&sample_event(3, NotificationPriority::High, "GoodApp")));
    }
}
