//! # Change Notification System
//!
//! Event-driven notification system for KVStore changes using a message bus pattern.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::debug;
use uuid::Uuid;

/// Types of changes that can occur in the store
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeType {
    Created,
    Updated,
    Deleted,
    Cleared,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Created => write!(f, "CREATED"),
            ChangeType::Updated => write!(f, "UPDATED"),
            ChangeType::Deleted => write!(f, "DELETED"),
            ChangeType::Cleared => write!(f, "CLEARED"),
        }
    }
}

/// Notification about a change in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeNotification {
    pub key: String,
    pub change_type: ChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub version: u64,
    pub timestamp: u64,
}

/// Unique identifier for a subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(Uuid);

impl SubscriptionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter for notifications
#[derive(Debug, Clone)]
pub enum NotificationFilter {
    /// All notifications
    All,
    /// Only notifications for specific key
    Key(String),
    /// Only notifications for keys with specific prefix
    KeyPrefix(String),
    /// Only notifications of specific type
    ChangeType(ChangeType),
    /// Combined filters (all must match)
    And(Vec<NotificationFilter>),
    /// Any of the filters can match
    Or(Vec<NotificationFilter>),
}

impl NotificationFilter {
    /// Check if a notification matches this filter
    pub fn matches(&self, notification: &ChangeNotification) -> bool {
        match self {
            NotificationFilter::All => true,
            NotificationFilter::Key(key) => notification.key == *key,
            NotificationFilter::KeyPrefix(prefix) => notification.key.starts_with(prefix),
            NotificationFilter::ChangeType(change_type) => notification.change_type == *change_type,
            NotificationFilter::And(filters) => filters.iter().all(|f| f.matches(notification)),
            NotificationFilter::Or(filters) => filters.iter().any(|f| f.matches(notification)),
        }
    }
}

/// Subscription to notifications
pub struct Subscription {
    pub id: SubscriptionId,
    pub filter: NotificationFilter,
    pub receiver: mpsc::UnboundedReceiver<ChangeNotification>,
}

/// Statistics about the notification bus
#[derive(Debug, Clone, Default)]
pub struct NotificationStats {
    pub total_notifications_sent: u64,
    pub total_subscribers: usize,
    pub dropped_notifications: u64,
}

/// Message bus for distributing change notifications
pub struct NotificationBus {
    /// Broadcast channel for all notifications
    broadcast_tx: broadcast::Sender<ChangeNotification>,

    /// Individual subscriber channels
    #[allow(clippy::type_complexity)]
    subscribers: Arc<
        RwLock<
            HashMap<
                SubscriptionId,
                (
                    NotificationFilter,
                    mpsc::UnboundedSender<ChangeNotification>,
                ),
            >,
        >,
    >,

    /// Statistics
    stats: Arc<RwLock<NotificationStats>>,
}

impl NotificationBus {
    /// Create a new notification bus
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            broadcast_tx,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NotificationStats::default())),
        }
    }

    /// Subscribe to notifications with a filter
    pub fn subscribe(&self, filter: NotificationFilter) -> Subscription {
        let id = SubscriptionId::new();
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut subscribers = self.subscribers.write();
            subscribers.insert(id, (filter.clone(), tx));
        }

        {
            let mut stats = self.stats.write();
            stats.total_subscribers += 1;
        }

        debug!(
            "New subscription created: {:?} with filter: {:?}",
            id, filter
        );

        Subscription {
            id,
            filter,
            receiver: rx,
        }
    }

    /// Subscribe to all notifications (convenience method)
    pub fn subscribe_all(&self) -> Subscription {
        self.subscribe(NotificationFilter::All)
    }

    /// Subscribe to notifications for a specific key
    pub fn subscribe_key(&self, key: &str) -> Subscription {
        self.subscribe(NotificationFilter::Key(key.to_string()))
    }

    /// Subscribe to notifications for keys with a specific prefix
    pub fn subscribe_prefix(&self, prefix: &str) -> Subscription {
        self.subscribe(NotificationFilter::KeyPrefix(prefix.to_string()))
    }

    /// Subscribe to notifications of a specific change type
    pub fn subscribe_change_type(&self, change_type: ChangeType) -> Subscription {
        self.subscribe(NotificationFilter::ChangeType(change_type))
    }

    /// Unsubscribe from notifications
    pub fn unsubscribe(&self, subscription_id: SubscriptionId) {
        let mut subscribers = self.subscribers.write();
        if subscribers.remove(&subscription_id).is_some() {
            let mut stats = self.stats.write();
            stats.total_subscribers = stats.total_subscribers.saturating_sub(1);
            debug!("Subscription removed: {:?}", subscription_id);
        }
    }

    /// Publish a notification to all subscribers
    pub async fn publish(&self, notification: ChangeNotification) {
        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.total_notifications_sent += 1;
        }

        // Send to broadcast channel (for global listeners)
        if self.broadcast_tx.send(notification.clone()).is_err() {
            // No broadcast receivers, that's fine
        }

        // Send to individual subscribers with filtering
        let subscribers = self.subscribers.read();
        let mut dropped_count = 0;

        for (filter, sender) in subscribers.values() {
            if filter.matches(&notification) && sender.send(notification.clone()).is_err() {
                // Subscriber channel is closed, will be cleaned up later
                dropped_count += 1;
            }
        }

        if dropped_count > 0 {
            let mut stats = self.stats.write();
            stats.dropped_notifications += dropped_count;
            debug!(
                "Dropped {} notifications due to closed channels",
                dropped_count
            );
        }

        debug!(
            "Published notification: key={}, type={:?}",
            notification.key, notification.change_type
        );
    }

    /// Get a broadcast receiver for all notifications
    pub fn broadcast_receiver(&self) -> broadcast::Receiver<ChangeNotification> {
        self.broadcast_tx.subscribe()
    }

    /// Get current statistics
    pub fn get_stats(&self) -> NotificationStats {
        let stats = self.stats.read();
        let subscribers = self.subscribers.read();

        NotificationStats {
            total_notifications_sent: stats.total_notifications_sent,
            total_subscribers: subscribers.len(),
            dropped_notifications: stats.dropped_notifications,
        }
    }

    /// Clean up closed subscriber channels
    pub fn cleanup_closed_subscribers(&self) {
        let mut subscribers = self.subscribers.write();
        let initial_count = subscribers.len();

        subscribers.retain(|_, (_, sender)| !sender.is_closed());

        let removed = initial_count - subscribers.len();
        if removed > 0 {
            debug!("Cleaned up {} closed subscriber channels", removed);
        }
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.read().len()
    }
}

impl Default for NotificationBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification listener that can be used for async processing
pub struct NotificationListener {
    subscription: Subscription,
    name: String,
}

impl NotificationListener {
    /// Create a new notification listener
    pub fn new(subscription: Subscription, name: String) -> Self {
        Self { subscription, name }
    }

    /// Start listening for notifications and process them with the given handler
    pub async fn listen<F, Fut>(&mut self, mut handler: F)
    where
        F: FnMut(ChangeNotification) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        debug!("Starting notification listener: {}", self.name);

        while let Some(notification) = self.subscription.receiver.recv().await {
            debug!(
                "Listener {} received notification: {:?}",
                self.name, notification
            );
            handler(notification).await;
        }

        debug!("Notification listener {} stopped", self.name);
    }

    /// Get the subscription ID
    pub fn subscription_id(&self) -> SubscriptionId {
        self.subscription.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_notification_bus_basic() {
        let bus = NotificationBus::new();
        let mut subscription = bus.subscribe_all();

        let notification = ChangeNotification {
            key: "test_key".to_string(),
            change_type: ChangeType::Created,
            old_value: None,
            new_value: Some("test_value".to_string()),
            version: 1,
            timestamp: 123456789,
        };

        // Publish notification
        bus.publish(notification.clone()).await;

        // Receive notification
        let received = subscription.receiver.recv().await.unwrap();
        assert_eq!(received.key, notification.key);
        assert_eq!(received.change_type, notification.change_type);
        assert_eq!(received.new_value, notification.new_value);
    }

    #[tokio::test]
    async fn test_notification_filtering() {
        let bus = NotificationBus::new();
        let mut key_subscription = bus.subscribe_key("specific_key");
        let mut prefix_subscription = bus.subscribe_prefix("prefix_");
        let mut type_subscription = bus.subscribe_change_type(ChangeType::Updated);

        // Notification that should match key filter
        let key_notification = ChangeNotification {
            key: "specific_key".to_string(),
            change_type: ChangeType::Created,
            old_value: None,
            new_value: Some("value".to_string()),
            version: 1,
            timestamp: 123456789,
        };

        // Notification that should match prefix filter
        let prefix_notification = ChangeNotification {
            key: "prefix_test".to_string(),
            change_type: ChangeType::Created,
            old_value: None,
            new_value: Some("value".to_string()),
            version: 2,
            timestamp: 123456790,
        };

        // Notification that should match type filter
        let type_notification = ChangeNotification {
            key: "any_key".to_string(),
            change_type: ChangeType::Updated,
            old_value: Some("old".to_string()),
            new_value: Some("new".to_string()),
            version: 3,
            timestamp: 123456791,
        };

        // Publish all notifications
        bus.publish(key_notification.clone()).await;
        bus.publish(prefix_notification.clone()).await;
        bus.publish(type_notification.clone()).await;

        // Check key subscription received only the key notification
        let received = key_subscription.receiver.recv().await.unwrap();
        assert_eq!(received.key, "specific_key");

        // Check prefix subscription received only the prefix notification
        let received = prefix_subscription.receiver.recv().await.unwrap();
        assert_eq!(received.key, "prefix_test");

        // Check type subscription received only the type notification
        let received = type_subscription.receiver.recv().await.unwrap();
        assert_eq!(received.change_type, ChangeType::Updated);
    }

    #[tokio::test]
    async fn test_notification_stats() {
        let bus = NotificationBus::new();
        let _subscription = bus.subscribe_all();

        let notification = ChangeNotification {
            key: "test".to_string(),
            change_type: ChangeType::Created,
            old_value: None,
            new_value: Some("value".to_string()),
            version: 1,
            timestamp: 123456789,
        };

        bus.publish(notification).await;

        let stats = bus.get_stats();
        assert_eq!(stats.total_notifications_sent, 1);
        assert_eq!(stats.total_subscribers, 1);
    }

    #[test]
    fn test_notification_filter_logic() {
        let notification = ChangeNotification {
            key: "test_key".to_string(),
            change_type: ChangeType::Updated,
            old_value: Some("old".to_string()),
            new_value: Some("new".to_string()),
            version: 1,
            timestamp: 123456789,
        };

        // Test individual filters
        assert!(NotificationFilter::All.matches(&notification));
        assert!(NotificationFilter::Key("test_key".to_string()).matches(&notification));
        assert!(!NotificationFilter::Key("other_key".to_string()).matches(&notification));
        assert!(NotificationFilter::KeyPrefix("test_".to_string()).matches(&notification));
        assert!(!NotificationFilter::KeyPrefix("other_".to_string()).matches(&notification));
        assert!(NotificationFilter::ChangeType(ChangeType::Updated).matches(&notification));
        assert!(!NotificationFilter::ChangeType(ChangeType::Created).matches(&notification));

        // Test AND filter
        let and_filter = NotificationFilter::And(vec![
            NotificationFilter::KeyPrefix("test_".to_string()),
            NotificationFilter::ChangeType(ChangeType::Updated),
        ]);
        assert!(and_filter.matches(&notification));

        // Test OR filter
        let or_filter = NotificationFilter::Or(vec![
            NotificationFilter::Key("wrong_key".to_string()),
            NotificationFilter::ChangeType(ChangeType::Updated),
        ]);
        assert!(or_filter.matches(&notification));
    }
}
