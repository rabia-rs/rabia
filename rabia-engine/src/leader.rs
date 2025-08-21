//! Simple leader selection logic for Rabia consensus protocol.
//!
//! In the Rabia protocol, leadership is not based on complex Raft-like elections with terms,
//! voting, and heartbeats. Instead, it uses simple deterministic selection based on cluster
//! membership: the leader is simply the first node in the sorted cluster view.
//!
//! When the cluster view changes (nodes join/leave), the leader is re-determined based on
//! the new cluster membership.

use rabia_core::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, info};

/// Simple leader selector that determines leadership based on cluster membership
#[derive(Debug, Clone)]
pub struct LeaderSelector {
    /// Current sorted cluster view
    cluster_view: Vec<NodeId>,
    /// Current leader (first node in sorted cluster view)
    current_leader: Option<NodeId>,
}

/// Information about the current leadership state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeadershipInfo {
    /// Current leader node ID
    pub leader: Option<NodeId>,
    /// All nodes in the cluster view
    pub cluster_nodes: Vec<NodeId>,
    /// Number of nodes in cluster
    pub cluster_size: usize,
}

impl LeaderSelector {
    /// Create a new leader selector with an empty cluster view
    pub fn new() -> Self {
        Self {
            cluster_view: Vec::new(),
            current_leader: None,
        }
    }

    /// Create a new leader selector with initial cluster nodes
    pub fn with_cluster(nodes: HashSet<NodeId>) -> Self {
        let mut selector = Self::new();
        selector.update_cluster_view(nodes);
        selector
    }

    /// Determine the leader from the current cluster view
    ///
    /// Returns the first node in the sorted cluster view, or None if empty
    pub fn determine_leader(&self) -> Option<NodeId> {
        self.cluster_view.first().copied()
    }

    /// Update the cluster view with new set of nodes
    ///
    /// Returns the new leader if it changed, or None if no change
    pub fn update_cluster_view(&mut self, nodes: HashSet<NodeId>) -> Option<NodeId> {
        // Convert to sorted vector for deterministic ordering
        let mut new_cluster_view: Vec<NodeId> = nodes.into_iter().collect();
        new_cluster_view.sort();

        let old_leader = self.current_leader;

        // Update cluster view
        self.cluster_view = new_cluster_view;
        self.current_leader = self.determine_leader();

        debug!(
            "Cluster view updated: {:?}, leader: {:?}",
            self.cluster_view, self.current_leader
        );

        // Return new leader if it changed
        if self.current_leader != old_leader {
            info!(
                "Leader changed from {:?} to {:?}",
                old_leader, self.current_leader
            );
            self.current_leader
        } else {
            None
        }
    }

    /// Add a node to the cluster view
    ///
    /// Returns the new leader if it changed
    pub fn add_node(&mut self, node_id: NodeId) -> Option<NodeId> {
        let mut nodes: HashSet<NodeId> = self.cluster_view.iter().copied().collect();
        nodes.insert(node_id);
        self.update_cluster_view(nodes)
    }

    /// Remove a node from the cluster view
    ///
    /// Returns the new leader if it changed
    pub fn remove_node(&mut self, node_id: NodeId) -> Option<NodeId> {
        let mut nodes: HashSet<NodeId> = self.cluster_view.iter().copied().collect();
        nodes.remove(&node_id);
        self.update_cluster_view(nodes)
    }

    /// Get the current leader
    pub fn get_leader(&self) -> Option<NodeId> {
        self.current_leader
    }

    /// Check if a specific node is the current leader
    pub fn is_leader(&self, node_id: NodeId) -> bool {
        self.current_leader == Some(node_id)
    }

    /// Get the current cluster view
    pub fn get_cluster_view(&self) -> &[NodeId] {
        &self.cluster_view
    }

    /// Get current leadership information
    pub fn get_leadership_info(&self) -> LeadershipInfo {
        LeadershipInfo {
            leader: self.current_leader,
            cluster_nodes: self.cluster_view.clone(),
            cluster_size: self.cluster_view.len(),
        }
    }

    /// Check if the cluster has any nodes
    pub fn has_nodes(&self) -> bool {
        !self.cluster_view.is_empty()
    }

    /// Get the number of nodes in the cluster
    pub fn cluster_size(&self) -> usize {
        self.cluster_view.len()
    }
}

impl Default for LeaderSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_empty_cluster_has_no_leader() {
        let selector = LeaderSelector::new();
        assert_eq!(selector.get_leader(), None);
        assert_eq!(selector.cluster_size(), 0);
        assert!(!selector.has_nodes());
    }

    #[test]
    fn test_single_node_becomes_leader() {
        let node1 = NodeId::from(1);
        let mut nodes = HashSet::new();
        nodes.insert(node1);

        let selector = LeaderSelector::with_cluster(nodes);
        assert_eq!(selector.get_leader(), Some(node1));
        assert!(selector.is_leader(node1));
        assert_eq!(selector.cluster_size(), 1);
    }

    #[test]
    fn test_multiple_nodes_first_becomes_leader() {
        let node1 = NodeId::from(1);
        let node2 = NodeId::from(2);
        let node3 = NodeId::from(3);

        let mut nodes = HashSet::new();
        nodes.insert(node3); // Insert out of order to test sorting
        nodes.insert(node1);
        nodes.insert(node2);

        let selector = LeaderSelector::with_cluster(nodes);

        // First node in sorted order should be leader
        assert_eq!(selector.get_leader(), Some(node1));
        assert!(selector.is_leader(node1));
        assert!(!selector.is_leader(node2));
        assert!(!selector.is_leader(node3));
        assert_eq!(selector.cluster_size(), 3);
    }

    #[test]
    fn test_leader_changes_when_cluster_changes() {
        let node1 = NodeId::from(1);
        let node2 = NodeId::from(2);
        let node3 = NodeId::from(3);

        let mut selector = LeaderSelector::new();

        // Start with node2 and node3
        let mut nodes = HashSet::new();
        nodes.insert(node2);
        nodes.insert(node3);

        let new_leader = selector.update_cluster_view(nodes);
        assert_eq!(new_leader, Some(node2)); // node2 is first
        assert_eq!(selector.get_leader(), Some(node2));

        // Add node1 (should become new leader as it's smallest)
        let new_leader = selector.add_node(node1);
        assert_eq!(new_leader, Some(node1));
        assert_eq!(selector.get_leader(), Some(node1));

        // Remove node1 (node2 should become leader again)
        let new_leader = selector.remove_node(node1);
        assert_eq!(new_leader, Some(node2));
        assert_eq!(selector.get_leader(), Some(node2));
    }

    #[test]
    fn test_no_leader_change_when_non_leader_leaves() {
        let node1 = NodeId::from(1);
        let node2 = NodeId::from(2);
        let node3 = NodeId::from(3);

        let mut nodes = HashSet::new();
        nodes.insert(node1);
        nodes.insert(node2);
        nodes.insert(node3);

        let mut selector = LeaderSelector::with_cluster(nodes);
        assert_eq!(selector.get_leader(), Some(node1)); // node1 is leader

        // Remove non-leader node3
        let new_leader = selector.remove_node(node3);
        assert_eq!(new_leader, None); // No leadership change
        assert_eq!(selector.get_leader(), Some(node1)); // Still node1
    }

    #[test]
    fn test_leadership_info() {
        let node1 = NodeId::from(1);
        let node2 = NodeId::from(2);

        let mut nodes = HashSet::new();
        nodes.insert(node1);
        nodes.insert(node2);

        let selector = LeaderSelector::with_cluster(nodes);
        let info = selector.get_leadership_info();

        assert_eq!(info.leader, Some(node1));
        assert_eq!(info.cluster_size, 2);
        assert!(info.cluster_nodes.contains(&node1));
        assert!(info.cluster_nodes.contains(&node2));
    }

    #[test]
    fn test_deterministic_ordering() {
        // Test that node ordering is deterministic regardless of insertion order
        let node1 = NodeId::from(5);
        let node2 = NodeId::from(1);
        let node3 = NodeId::from(3);

        // First insertion order
        let mut nodes1 = HashSet::new();
        nodes1.insert(node1);
        nodes1.insert(node2);
        nodes1.insert(node3);
        let selector1 = LeaderSelector::with_cluster(nodes1);

        // Second insertion order
        let mut nodes2 = HashSet::new();
        nodes2.insert(node3);
        nodes2.insert(node1);
        nodes2.insert(node2);
        let selector2 = LeaderSelector::with_cluster(nodes2);

        // Should have same leader and cluster view
        assert_eq!(selector1.get_leader(), selector2.get_leader());
        assert_eq!(selector1.get_cluster_view(), selector2.get_cluster_view());
    }
}
