//! Cluster topology management for leader coordination.

use crate::{LeaderError, LeaderResult};
use rabia_core::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Information about a cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub node_id: NodeId,

    /// Network address for communication
    pub address: SocketAddr,

    /// Node role in the cluster
    pub role: NodeRole,

    /// Node capabilities and metadata
    pub metadata: HashMap<String, String>,

    /// When the node joined the cluster
    pub joined_at: u64,

    /// Last time we heard from this node
    pub last_seen: u64,

    /// Node version information
    pub version: String,
}

/// Role of a node in the cluster
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeRole {
    /// Full consensus participant
    Participant,

    /// Observer node (read-only)
    Observer,

    /// Learner node (catching up)
    Learner,

    /// Standby node (backup)
    Standby,
}

/// Current cluster topology
#[derive(Debug, Clone)]
pub struct ClusterTopology {
    /// All known nodes in the cluster
    pub nodes: HashMap<NodeId, NodeInfo>,

    /// Nodes currently considered healthy
    pub healthy_nodes: HashSet<NodeId>,

    /// Cluster configuration version
    pub version: u64,

    /// When this topology was last updated
    pub updated_at: u64,
}

/// Configuration for topology management
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    /// How often to check node health
    pub health_check_interval: Duration,

    /// Time before considering a node unhealthy
    pub node_timeout: Duration,

    /// Maximum number of nodes in cluster
    pub max_nodes: usize,

    /// Require minimum quorum size
    pub min_quorum_size: usize,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(5),
            node_timeout: Duration::from_secs(30),
            max_nodes: 100,
            min_quorum_size: 3,
        }
    }
}

/// Statistics about topology operations
#[derive(Debug, Default, Clone)]
pub struct TopologyStats {
    pub nodes_added: u64,
    pub nodes_removed: u64,
    pub health_checks: u64,
    pub topology_changes: u64,
    pub quorum_losses: u64,
}

/// Manages cluster topology and node membership
pub struct TopologyManager {
    config: TopologyConfig,
    topology: Arc<RwLock<ClusterTopology>>,
    stats: Arc<RwLock<TopologyStats>>,
}

impl TopologyManager {
    /// Create a new topology manager
    pub async fn new() -> LeaderResult<Self> {
        let config = TopologyConfig::default();

        let topology = ClusterTopology {
            nodes: HashMap::new(),
            healthy_nodes: HashSet::new(),
            version: 0,
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        Ok(Self {
            config,
            topology: Arc::new(RwLock::new(topology)),
            stats: Arc::new(RwLock::new(TopologyStats::default())),
        })
    }

    /// Create with custom configuration
    pub async fn with_config(config: TopologyConfig) -> LeaderResult<Self> {
        let topology = ClusterTopology {
            nodes: HashMap::new(),
            healthy_nodes: HashSet::new(),
            version: 0,
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };

        Ok(Self {
            config,
            topology: Arc::new(RwLock::new(topology)),
            stats: Arc::new(RwLock::new(TopologyStats::default())),
        })
    }

    /// Add a node to the cluster topology
    pub async fn add_node(&self, node_info: NodeInfo) -> LeaderResult<()> {
        let mut topology = self.topology.write().await;

        if topology.nodes.len() >= self.config.max_nodes {
            return Err(LeaderError::TopologyError {
                reason: format!("Maximum nodes limit reached: {}", self.config.max_nodes),
            });
        }

        let node_id = node_info.node_id;

        info!("Adding node {} to cluster topology", node_id);

        topology.nodes.insert(node_id, node_info);
        topology.healthy_nodes.insert(node_id);
        topology.version += 1;
        topology.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.nodes_added += 1;
            stats.topology_changes += 1;
        }

        debug!("Cluster topology now has {} nodes", topology.nodes.len());

        Ok(())
    }

    /// Remove a node from the cluster topology
    pub async fn remove_node(&self, node_id: NodeId) -> LeaderResult<()> {
        let mut topology = self.topology.write().await;

        if topology.nodes.remove(&node_id).is_some() {
            info!("Removing node {} from cluster topology", node_id);

            topology.healthy_nodes.remove(&node_id);
            topology.version += 1;
            topology.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.nodes_removed += 1;
                stats.topology_changes += 1;
            }

            // Check if we still have quorum
            if topology.healthy_nodes.len() < self.config.min_quorum_size {
                warn!(
                    "Quorum lost: {} healthy nodes < {} required",
                    topology.healthy_nodes.len(),
                    self.config.min_quorum_size
                );

                let mut stats = self.stats.write().await;
                stats.quorum_losses += 1;
            }
        }

        Ok(())
    }

    /// Update node health status
    pub async fn update_node_health(&self, node_id: NodeId, is_healthy: bool) -> LeaderResult<()> {
        let mut topology = self.topology.write().await;

        if let Some(node_info) = topology.nodes.get_mut(&node_id) {
            node_info.last_seen = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            let was_healthy = topology.healthy_nodes.contains(&node_id);

            if is_healthy && !was_healthy {
                debug!("Node {} marked as healthy", node_id);
                topology.healthy_nodes.insert(node_id);
                topology.version += 1;
                topology.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
            } else if !is_healthy && was_healthy {
                warn!("Node {} marked as unhealthy", node_id);
                topology.healthy_nodes.remove(&node_id);
                topology.version += 1;
                topology.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                // Check quorum
                if topology.healthy_nodes.len() < self.config.min_quorum_size {
                    warn!("Quorum lost after marking node {} unhealthy", node_id);
                    let mut stats = self.stats.write().await;
                    stats.quorum_losses += 1;
                }
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.health_checks += 1;
        }

        Ok(())
    }

    /// Get current cluster topology
    pub async fn get_topology(&self) -> ClusterTopology {
        self.topology.read().await.clone()
    }

    /// Get list of healthy nodes
    pub async fn get_healthy_nodes(&self) -> LeaderResult<Vec<NodeId>> {
        let topology = self.topology.read().await;
        Ok(topology.healthy_nodes.iter().copied().collect())
    }

    /// Get all nodes (healthy and unhealthy)
    pub async fn get_all_nodes(&self) -> Vec<NodeId> {
        let topology = self.topology.read().await;
        topology.nodes.keys().copied().collect()
    }

    /// Get node information
    pub async fn get_node_info(&self, node_id: NodeId) -> Option<NodeInfo> {
        let topology = self.topology.read().await;
        topology.nodes.get(&node_id).cloned()
    }

    /// Check if cluster has quorum
    pub async fn has_quorum(&self) -> bool {
        let topology = self.topology.read().await;
        topology.healthy_nodes.len() >= self.config.min_quorum_size
    }

    /// Get cluster size (total nodes)
    pub async fn cluster_size(&self) -> usize {
        let topology = self.topology.read().await;
        topology.nodes.len()
    }

    /// Get healthy node count
    pub async fn healthy_node_count(&self) -> usize {
        let topology = self.topology.read().await;
        topology.healthy_nodes.len()
    }

    /// Perform health check on all nodes
    pub async fn health_check(&self) -> LeaderResult<()> {
        let topology = self.topology.read().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let node_timeout = self.config.node_timeout.as_millis() as u64;

        let mut unhealthy_nodes = Vec::new();

        for (&node_id, node_info) in &topology.nodes {
            if now.saturating_sub(node_info.last_seen) > node_timeout {
                unhealthy_nodes.push(node_id);
            }
        }

        drop(topology);

        // Mark unhealthy nodes
        for node_id in unhealthy_nodes {
            self.update_node_health(node_id, false).await?;
        }

        Ok(())
    }

    /// Get topology statistics
    pub async fn get_stats(&self) -> TopologyStats {
        self.stats.read().await.clone()
    }

    /// Start background health monitoring
    pub async fn start_health_monitoring(self: Arc<Self>) -> LeaderResult<()> {
        let health_check_interval = self.config.health_check_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health_check_interval);

            loop {
                interval.tick().await;

                if let Err(e) = self.health_check().await {
                    warn!("Health check failed: {}", e);
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn create_test_node_info(id: u64, port: u16) -> NodeInfo {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        NodeInfo {
            node_id: NodeId::from(id),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            role: NodeRole::Participant,
            metadata: HashMap::new(),
            joined_at: now,
            last_seen: now,
            version: "0.2.0".to_string(),
        }
    }

    #[tokio::test]
    async fn test_topology_manager_creation() {
        let manager = TopologyManager::new().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_add_node() {
        let manager = TopologyManager::new().await.unwrap();
        let node_info = create_test_node_info(1, 8001);

        let result = manager.add_node(node_info.clone()).await;
        assert!(result.is_ok());

        let topology = manager.get_topology().await;
        assert_eq!(topology.nodes.len(), 1);
        assert!(topology.healthy_nodes.contains(&node_info.node_id));
    }

    #[tokio::test]
    async fn test_remove_node() {
        let manager = TopologyManager::new().await.unwrap();
        let node_info = create_test_node_info(1, 8001);
        let node_id = node_info.node_id;

        manager.add_node(node_info).await.unwrap();
        manager.remove_node(node_id).await.unwrap();

        let topology = manager.get_topology().await;
        assert_eq!(topology.nodes.len(), 0);
        assert!(!topology.healthy_nodes.contains(&node_id));
    }

    #[tokio::test]
    async fn test_health_updates() {
        let manager = TopologyManager::new().await.unwrap();
        let node_info = create_test_node_info(1, 8001);
        let node_id = node_info.node_id;

        manager.add_node(node_info).await.unwrap();

        // Mark as unhealthy
        manager.update_node_health(node_id, false).await.unwrap();
        let topology = manager.get_topology().await;
        assert!(!topology.healthy_nodes.contains(&node_id));

        // Mark as healthy again
        manager.update_node_health(node_id, true).await.unwrap();
        let topology = manager.get_topology().await;
        assert!(topology.healthy_nodes.contains(&node_id));
    }

    #[tokio::test]
    async fn test_quorum_check() {
        let config = TopologyConfig {
            min_quorum_size: 3,
            ..Default::default()
        };
        let manager = TopologyManager::with_config(config).await.unwrap();

        // No quorum with 0 nodes
        assert!(!manager.has_quorum().await);

        // Add nodes
        for i in 1..=3 {
            let node_info = create_test_node_info(i, 8000 + i as u16);
            manager.add_node(node_info).await.unwrap();
        }

        // Now has quorum
        assert!(manager.has_quorum().await);
    }
}
