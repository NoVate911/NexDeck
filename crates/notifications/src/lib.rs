//! Centralized notification center domain types, filtering, and sleep buffering.
//!
//! Events received while the host display is off or the system is
//! transitioning/disconnected are retained in a bounded queue for delivery once
//! connectivity resumes.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Priority classification for notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum NotificationPriority {
    /// Informational low-urgency event.
    Low,
    /// Standard application or system notification.
    #[default]
    Normal,
    /// Actionable warning (e.g., high disk utilization).
    High,
    /// Immediate attention required (e.g., thermal threshold breach).
    Critical,
}

/// Origin category for a notification event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name")]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
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
    /// Trims, sorts, and deduplicates `muted_origins` entries.
    pub fn normalize(&mut self) {
        self.muted_origins = self
            .muted_origins
            .iter()
            .map(|origin| origin.trim().to_owned())
            .filter(|origin| !origin.is_empty())
            .collect();
        self.muted_origins.sort();
        self.muted_origins.dedup();
    }

    /// Returns `true` if `event` satisfies the filter criteria.
    #[must_use]
    pub fn matches(&self, event: &NotificationEvent) -> bool {
        if event.priority < self.min_priority {
            return false;
        }
        match &event.source {
            NotificationSource::Application(name) | NotificationSource::Plugin(name) => !self
                .muted_origins
                .iter()
                .any(|muted| muted.eq_ignore_ascii_case(name)),
            NotificationSource::WindowsSystem | NotificationSource::HardwareAlert => true,
        }
    }
}

/// Control operations sent over WSS (`MessageKind::Notification`) to synchronize
/// dismissal, queue clearing, or filter updates between mobile client and agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum NotificationSyncCommand {
    /// Dismisses a single notification event by ID.
    Dismiss {
        /// Identifier of the event to remove.
        event_id: u64,
    },
    /// Clears all buffered notifications from the host queue.
    ClearAll,
    /// Updates active notification filter settings on the host.
    UpdateFilter {
        /// Updated priority threshold and muted origins list.
        filter: NotificationFilter,
    },
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

    /// Rebuilds a bounded queue from persisted events, keeping up to `capacity` newest items.
    #[must_use]
    pub fn from_items(capacity: usize, items: impl IntoIterator<Item = NotificationEvent>) -> Self {
        let mut queue = Self::new(capacity);
        for event in items {
            queue.push(event);
        }
        queue
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

    /// Returns a non-destructive FIFO snapshot of currently buffered notifications.
    #[must_use]
    pub fn items(&self) -> Vec<NotificationEvent> {
        self.items.iter().cloned().collect()
    }

    /// Returns buffered notifications matching `filter` in FIFO order.
    #[must_use]
    pub fn items_matching(&self, filter: &NotificationFilter) -> Vec<NotificationEvent> {
        self.items
            .iter()
            .filter(|event| filter.matches(event))
            .cloned()
            .collect()
    }

    /// Removes the notification with matching `event_id` if present.
    pub fn remove_by_id(&mut self, event_id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|event| event.id != event_id);
        self.items.len() != before
    }

    /// Clears all buffered notifications and returns the number of removed entries.
    pub fn clear(&mut self) -> usize {
        let count = self.items.len();
        self.items.clear();
        count
    }

    /// Suggests the next unique event identifier strictly above all buffered IDs.
    #[must_use]
    pub fn next_id_hint(&self) -> u64 {
        self.items
            .iter()
            .map(|event| event.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
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
        queue.push(sample_event(1, NotificationPriority::Normal, "ExampleAppA"));
        queue.push(sample_event(2, NotificationPriority::High, "ExampleAppA"));
        queue.push(sample_event(
            3,
            NotificationPriority::Critical,
            "ExampleAppB",
        ));

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
            muted_origins: vec!["MutedExampleApp".to_owned()],
        };

        assert!(!filter.matches(&sample_event(
            1,
            NotificationPriority::Normal,
            "AllowedExampleApp"
        )));
        assert!(!filter.matches(&sample_event(
            2,
            NotificationPriority::Critical,
            "MutedExampleApp"
        )));
        assert!(filter.matches(&sample_event(
            3,
            NotificationPriority::High,
            "AllowedExampleApp"
        )));
    }

    #[test]
    fn queue_supports_reload_filtering_dismiss_clear_and_sync_commands() {
        let seeded = vec![
            sample_event(10, NotificationPriority::Low, "ExampleAppA"),
            sample_event(11, NotificationPriority::High, "MutedApp"),
            sample_event(12, NotificationPriority::Critical, "ExampleAppB"),
        ];
        let mut queue = NotificationQueue::from_items(4, seeded);
        assert_eq!(queue.next_id_hint(), 13);

        let mut filter = NotificationFilter {
            min_priority: NotificationPriority::Normal,
            muted_origins: vec!["  mutedapp  ".to_owned(), "mutedapp".to_owned()],
        };
        filter.normalize();
        assert_eq!(filter.muted_origins, vec!["mutedapp".to_owned()]);

        let matching = queue.items_matching(&filter);
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, 12);

        assert!(queue.remove_by_id(10));
        assert!(!queue.remove_by_id(999));
        assert_eq!(queue.clear(), 2);
        assert!(queue.is_empty());

        let cmd = NotificationSyncCommand::Dismiss { event_id: 12 };
        assert_eq!(cmd, NotificationSyncCommand::Dismiss { event_id: 12 });
    }
}
