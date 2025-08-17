use async_trait::async_trait;
use std::collections::HashSet;
use crate::{Result, NodeId};
use crate::messages::ProtocolMessage;

#[derive(Debug, Clone)]
pub struct ClusterConfig {
    pub node_id: NodeId,
    pub all_nodes: HashSet<NodeId>,
    pub quorum_size: usize,
}

impl ClusterConfig {
    pub fn new(node_id: NodeId, all_nodes: HashSet<NodeId>) -> Self {
        let quorum_size = (all_nodes.len() / 2) + 1;
        Self {
            node_id,
            all_nodes,
            quorum_size,
        }
    }

    pub fn has_quorum(&self, active_nodes: &HashSet<NodeId>) -> bool {
        active_nodes.len() >= self.quorum_size
    }

    pub fn is_majority(&self, count: usize) -> bool {
        count >= self.quorum_size
    }

    pub fn total_nodes(&self) -> usize {
        self.all_nodes.len()
    }
}

#[async_trait]
pub trait NetworkTransport: Send + Sync {
    async fn send_to(&self, target: NodeId, message: ProtocolMessage) -> Result<()>;

    async fn broadcast(&self, message: ProtocolMessage, exclude: Option<NodeId>) -> Result<()>;

    async fn receive(&mut self) -> Result<(NodeId, ProtocolMessage)>;

    async fn get_connected_nodes(&self) -> Result<HashSet<NodeId>>;

    async fn is_connected(&self, node_id: NodeId) -> Result<bool>;

    async fn disconnect(&mut self) -> Result<()>;

    async fn reconnect(&mut self) -> Result<()>;
}

#[async_trait]
pub trait NetworkEventHandler: Send + Sync {
    async fn on_node_connected(&self, node_id: NodeId);
    
    async fn on_node_disconnected(&self, node_id: NodeId);
    
    async fn on_network_partition(&self, active_nodes: HashSet<NodeId>);
    
    async fn on_quorum_lost(&self);
    
    async fn on_quorum_restored(&self, active_nodes: HashSet<NodeId>);
}

pub struct NetworkMonitor {
    config: ClusterConfig,
    connected_nodes: HashSet<NodeId>,
    has_quorum: bool,
}

impl NetworkMonitor {
    pub fn new(config: ClusterConfig) -> Self {
        let connected_nodes = config.all_nodes.clone();
        let has_quorum = config.has_quorum(&connected_nodes);
        
        Self {
            config,
            connected_nodes,
            has_quorum,
        }
    }

    pub fn update_connected_nodes(&mut self, nodes: HashSet<NodeId>) -> Vec<NetworkEvent> {
        let mut events = Vec::new();
        
        let newly_connected: HashSet<_> = nodes.difference(&self.connected_nodes).copied().collect();
        let newly_disconnected: HashSet<_> = self.connected_nodes.difference(&nodes).copied().collect();
        
        for &node in &newly_connected {
            events.push(NetworkEvent::NodeConnected(node));
        }
        
        for &node in &newly_disconnected {
            events.push(NetworkEvent::NodeDisconnected(node));
        }
        
        let new_has_quorum = self.config.has_quorum(&nodes);
        
        if self.has_quorum && !new_has_quorum {
            events.push(NetworkEvent::QuorumLost);
        } else if !self.has_quorum && new_has_quorum {
            events.push(NetworkEvent::QuorumRestored(nodes.clone()));
        }
        
        if !newly_connected.is_empty() || !newly_disconnected.is_empty() {
            events.push(NetworkEvent::NetworkPartition(nodes.clone()));
        }
        
        self.connected_nodes = nodes;
        self.has_quorum = new_has_quorum;
        
        events
    }

    pub fn has_quorum(&self) -> bool {
        self.has_quorum
    }

    pub fn connected_nodes(&self) -> &HashSet<NodeId> {
        &self.connected_nodes
    }

    pub fn quorum_size(&self) -> usize {
        self.config.quorum_size
    }
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    NodeConnected(NodeId),
    NodeDisconnected(NodeId),
    NetworkPartition(HashSet<NodeId>),
    QuorumLost,
    QuorumRestored(HashSet<NodeId>),
}