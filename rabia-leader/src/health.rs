//! Node health monitoring and status tracking.

use crate::LeaderResult;
use rabia_core::NodeId;
// Note: removed serde derives since we're using timestamps instead of Instant
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

/// Health status of a node
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// Node is healthy and responsive
    Healthy,

    /// Node is degraded but operational
    Degraded {
        /// Reason for degradation
        reason: String,
        /// Severity level (0-100, higher = more severe)
        severity: u8,
    },

    /// Node is unhealthy and not responsive
    Unhealthy {
        /// Reason for unhealthy status
        reason: String,
        /// When the node became unhealthy
        since: u64,
    },

    /// Node status is unknown
    Unknown,
}

/// Detailed health information for a node
#[derive(Debug, Clone)]
pub struct NodeHealth {
    /// Node identifier
    pub node_id: NodeId,

    /// Current health status
    pub status: HealthStatus,

    /// Last successful health check
    pub last_check: u64,

    /// Response time for last health check
    pub response_time: Duration,

    /// Number of consecutive failed checks
    pub consecutive_failures: u32,

    /// Number of consecutive successful checks
    pub consecutive_successes: u32,

    /// Total number of health checks performed
    pub total_checks: u64,

    /// Health check success rate (0.0 - 1.0)
    pub success_rate: f64,

    /// Additional health metrics
    pub metrics: HashMap<String, f64>,
}

/// Configuration for health monitoring
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Interval between health checks
    pub check_interval: Duration,

    /// Timeout for health check responses
    pub check_timeout: Duration,

    /// Number of failed checks before marking unhealthy
    pub failure_threshold: u32,

    /// Number of successful checks before marking healthy
    pub recovery_threshold: u32,

    /// Enable detailed health metrics collection
    pub detailed_metrics: bool,

    /// Maximum response time before marking as degraded
    pub degraded_threshold: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            check_timeout: Duration::from_secs(3),
            failure_threshold: 3,
            recovery_threshold: 2,
            detailed_metrics: true,
            degraded_threshold: Duration::from_millis(1000),
        }
    }
}

/// Statistics about health monitoring
#[derive(Debug, Default, Clone)]
pub struct HealthStats {
    pub total_checks: u64,
    pub successful_checks: u64,
    pub failed_checks: u64,
    pub timeouts: u64,
    pub nodes_marked_unhealthy: u64,
    pub nodes_recovered: u64,
    pub average_response_time: Duration,
}

/// Monitors node health in the cluster
pub struct HealthMonitor {
    config: HealthConfig,
    node_health: Arc<RwLock<HashMap<NodeId, NodeHealth>>>,
    stats: Arc<RwLock<HealthStats>>,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub async fn new() -> LeaderResult<Self> {
        let config = HealthConfig::default();

        Ok(Self {
            config,
            node_health: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(HealthStats::default())),
        })
    }

    /// Create with custom configuration
    pub async fn with_config(config: HealthConfig) -> LeaderResult<Self> {
        Ok(Self {
            config,
            node_health: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(HealthStats::default())),
        })
    }

    /// Register a node for health monitoring
    pub async fn register_node(&self, node_id: NodeId) -> LeaderResult<()> {
        let mut node_health = self.node_health.write().await;

        let health = NodeHealth {
            node_id,
            status: HealthStatus::Unknown,
            last_check: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            response_time: Duration::from_millis(0),
            consecutive_failures: 0,
            consecutive_successes: 0,
            total_checks: 0,
            success_rate: 0.0,
            metrics: HashMap::new(),
        };

        node_health.insert(node_id, health);
        debug!("Registered node {} for health monitoring", node_id);

        Ok(())
    }

    /// Unregister a node from health monitoring
    pub async fn unregister_node(&self, node_id: NodeId) -> LeaderResult<()> {
        let mut node_health = self.node_health.write().await;
        node_health.remove(&node_id);
        debug!("Unregistered node {} from health monitoring", node_id);

        Ok(())
    }

    /// Perform health check on a specific node
    pub async fn check_node_health(&self, node_id: NodeId) -> LeaderResult<HealthStatus> {
        let start_time = Instant::now();

        // Simulate health check (in practice, this would ping the node)
        let check_result = self.perform_health_check(node_id).await;
        let response_time = start_time.elapsed();

        // Update node health based on check result
        self.update_node_health(node_id, check_result, response_time)
            .await?;

        // Get updated status
        let node_health = self.node_health.read().await;
        if let Some(health) = node_health.get(&node_id) {
            Ok(health.status.clone())
        } else {
            Ok(HealthStatus::Unknown)
        }
    }

    /// Get health status for a specific node
    pub async fn get_node_health(&self, node_id: NodeId) -> Option<NodeHealth> {
        let node_health = self.node_health.read().await;
        node_health.get(&node_id).cloned()
    }

    /// Get health status for all monitored nodes
    pub async fn get_all_health(&self) -> HashMap<NodeId, NodeHealth> {
        self.node_health.read().await.clone()
    }

    /// Get list of healthy nodes
    pub async fn get_healthy_nodes(&self) -> Vec<NodeId> {
        let node_health = self.node_health.read().await;
        node_health
            .iter()
            .filter(|(_, health)| matches!(health.status, HealthStatus::Healthy))
            .map(|(&node_id, _)| node_id)
            .collect()
    }

    /// Get list of unhealthy nodes
    pub async fn get_unhealthy_nodes(&self) -> Vec<NodeId> {
        let node_health = self.node_health.read().await;
        node_health
            .iter()
            .filter(|(_, health)| matches!(health.status, HealthStatus::Unhealthy { .. }))
            .map(|(&node_id, _)| node_id)
            .collect()
    }

    /// Check if a node is healthy
    pub async fn is_node_healthy(&self, node_id: NodeId) -> bool {
        let node_health = self.node_health.read().await;
        if let Some(health) = node_health.get(&node_id) {
            matches!(health.status, HealthStatus::Healthy)
        } else {
            false
        }
    }

    /// Get health monitoring statistics
    pub async fn get_stats(&self) -> HealthStats {
        self.stats.read().await.clone()
    }

    /// Start continuous health monitoring
    pub async fn start_monitoring(self: Arc<Self>) -> LeaderResult<()> {
        let node_health = Arc::clone(&self.node_health);
        let _stats = Arc::clone(&self.stats);
        let check_interval = self.config.check_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);

            loop {
                interval.tick().await;

                // Get list of nodes to check
                let nodes: Vec<NodeId> = {
                    let health_map = node_health.read().await;
                    health_map.keys().copied().collect()
                };

                // Check health of all registered nodes
                for node_id in nodes {
                    if let Err(e) = self.check_node_health(node_id).await {
                        error!("Health check failed for node {}: {}", node_id, e);
                    }
                }
            }
        });

        Ok(())
    }

    // Private methods

    async fn perform_health_check(&self, _node_id: NodeId) -> HealthCheckResult {
        // Simulate a health check
        // In practice, this would:
        // 1. Send a ping/health request to the node
        // 2. Wait for response within timeout
        // 3. Validate the response
        // 4. Collect additional metrics if enabled

        tokio::time::sleep(Duration::from_millis(10)).await; // Simulate network delay

        // For demonstration, randomly succeed/fail
        let success = rand::random::<f64>() > 0.1; // 90% success rate

        if success {
            let mut metrics = HashMap::new();
            if self.config.detailed_metrics {
                metrics.insert("cpu_usage".to_string(), rand::random::<f64>() * 100.0);
                metrics.insert("memory_usage".to_string(), rand::random::<f64>() * 100.0);
                metrics.insert("disk_usage".to_string(), rand::random::<f64>() * 100.0);
            }

            HealthCheckResult::Success { metrics }
        } else {
            HealthCheckResult::Failure {
                reason: "Node not responding".to_string(),
            }
        }
    }

    async fn update_node_health(
        &self,
        node_id: NodeId,
        check_result: HealthCheckResult,
        response_time: Duration,
    ) -> LeaderResult<()> {
        let mut node_health = self.node_health.write().await;
        let mut stats = self.stats.write().await;

        if let Some(health) = node_health.get_mut(&node_id) {
            health.last_check = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            health.response_time = response_time;
            health.total_checks += 1;

            match check_result {
                HealthCheckResult::Success { metrics } => {
                    health.consecutive_failures = 0;
                    health.consecutive_successes += 1;
                    health.metrics = metrics;

                    // Update status based on response time and consecutive successes
                    health.status = if response_time > self.config.degraded_threshold {
                        HealthStatus::Degraded {
                            reason: format!("High response time: {:?}", response_time),
                            severity: 50,
                        }
                    } else if health.consecutive_successes >= self.config.recovery_threshold {
                        if !matches!(health.status, HealthStatus::Healthy) {
                            debug!("Node {} recovered and marked as healthy", node_id);
                            stats.nodes_recovered += 1;
                        }
                        HealthStatus::Healthy
                    } else {
                        health.status.clone() // Keep current status during recovery
                    };

                    stats.successful_checks += 1;
                }
                HealthCheckResult::Failure { reason } => {
                    health.consecutive_successes = 0;
                    health.consecutive_failures += 1;

                    // Update status based on consecutive failures
                    if health.consecutive_failures >= self.config.failure_threshold {
                        if !matches!(health.status, HealthStatus::Unhealthy { .. }) {
                            warn!("Node {} marked as unhealthy: {}", node_id, reason);
                            stats.nodes_marked_unhealthy += 1;
                        }
                        health.status = HealthStatus::Unhealthy {
                            reason: reason.clone(),
                            since: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                        };
                    }

                    stats.failed_checks += 1;
                }
                HealthCheckResult::Timeout => {
                    health.consecutive_successes = 0;
                    health.consecutive_failures += 1;

                    if health.consecutive_failures >= self.config.failure_threshold {
                        health.status = HealthStatus::Unhealthy {
                            reason: "Health check timeout".to_string(),
                            since: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                        };
                    }

                    stats.timeouts += 1;
                    stats.failed_checks += 1;
                }
            }

            // Update success rate
            health.success_rate = if health.total_checks > 0 {
                (health.total_checks - (health.consecutive_failures as u64)) as f64
                    / health.total_checks as f64
            } else {
                0.0
            };
        }

        stats.total_checks += 1;

        // Update average response time
        if stats.successful_checks > 0 {
            let total_time = stats.average_response_time.as_millis() as u64
                * (stats.successful_checks - 1)
                + response_time.as_millis() as u64;
            stats.average_response_time =
                Duration::from_millis(total_time / stats.successful_checks);
        }

        Ok(())
    }
}

#[derive(Debug)]
#[allow(dead_code)]
enum HealthCheckResult {
    Success { metrics: HashMap<String, f64> },
    Failure { reason: String },
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let monitor = HealthMonitor::new().await;
        assert!(monitor.is_ok());
    }

    #[tokio::test]
    async fn test_register_node() {
        let monitor = HealthMonitor::new().await.unwrap();
        let node_id = NodeId::from(1);

        let result = monitor.register_node(node_id).await;
        assert!(result.is_ok());

        let health = monitor.get_node_health(node_id).await;
        assert!(health.is_some());
    }

    #[tokio::test]
    async fn test_health_check() {
        let monitor = HealthMonitor::new().await.unwrap();
        let node_id = NodeId::from(1);

        monitor.register_node(node_id).await.unwrap();

        let status = monitor.check_node_health(node_id).await;
        assert!(status.is_ok());
    }

    #[tokio::test]
    async fn test_unregister_node() {
        let monitor = HealthMonitor::new().await.unwrap();
        let node_id = NodeId::from(1);

        monitor.register_node(node_id).await.unwrap();
        monitor.unregister_node(node_id).await.unwrap();

        let health = monitor.get_node_health(node_id).await;
        assert!(health.is_none());
    }
}
