use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::sleep;
use rand::Rng;
use tracing::{debug, warn, info};

use rabia_core::{
    NodeId, Result, RabiaError,
    messages::ProtocolMessage,
    network::NetworkTransport,
};

#[derive(Debug, Clone)]
pub struct NetworkConditions {
    pub latency_min: Duration,
    pub latency_max: Duration,
    pub packet_loss_rate: f64,
    pub partition_probability: f64,
    pub bandwidth_limit: Option<usize>, // bytes per second
}

impl Default for NetworkConditions {
    fn default() -> Self {
        Self {
            latency_min: Duration::from_millis(1),
            latency_max: Duration::from_millis(10),
            packet_loss_rate: 0.0,
            partition_probability: 0.0,
            bandwidth_limit: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkPartition {
    pub partitioned_nodes: HashSet<NodeId>,
    pub duration: Duration,
    pub started_at: Instant,
}

#[derive(Debug)]
struct PendingMessage {
    from: NodeId,
    to: NodeId,
    message: ProtocolMessage,
    deliver_at: Instant,
    size_bytes: usize,
}

pub struct NetworkSimulator {
    nodes: Arc<RwLock<HashMap<NodeId, mpsc::UnboundedSender<(NodeId, ProtocolMessage)>>>>,
    conditions: Arc<RwLock<NetworkConditions>>,
    partitions: Arc<RwLock<Vec<NetworkPartition>>>,
    pending_messages: Arc<Mutex<VecDeque<PendingMessage>>>,
    message_stats: Arc<Mutex<NetworkStats>>,
    shutdown: Arc<Mutex<bool>>,
}

#[derive(Debug, Default, Clone)]
pub struct NetworkStats {
    pub messages_sent: u64,
    pub messages_delivered: u64,
    pub messages_dropped: u64,
    pub total_latency: Duration,
    pub total_bytes: usize,
}

impl NetworkStats {
    pub fn average_latency(&self) -> Duration {
        if self.messages_delivered > 0 {
            self.total_latency / self.messages_delivered as u32
        } else {
            Duration::ZERO
        }
    }

    pub fn throughput_mbps(&self, duration: Duration) -> f64 {
        if duration.as_secs_f64() > 0.0 {
            (self.total_bytes as f64 * 8.0) / (duration.as_secs_f64() * 1_000_000.0)
        } else {
            0.0
        }
    }
}

impl NetworkSimulator {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            conditions: Arc::new(RwLock::new(NetworkConditions::default())),
            partitions: Arc::new(RwLock::new(Vec::new())),
            pending_messages: Arc::new(Mutex::new(VecDeque::new())),
            message_stats: Arc::new(Mutex::new(NetworkStats::default())),
            shutdown: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn add_node(&self, node_id: NodeId) -> mpsc::UnboundedReceiver<(NodeId, ProtocolMessage)> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.nodes.write().await.insert(node_id, tx);
        info!("Added node {} to network simulation", node_id);
        rx
    }

    pub async fn remove_node(&self, node_id: NodeId) {
        self.nodes.write().await.remove(&node_id);
        info!("Removed node {} from network simulation", node_id);
    }

    pub async fn update_conditions(&self, conditions: NetworkConditions) {
        debug!("Updated network conditions: {:?}", conditions);
        *self.conditions.write().await = conditions;
    }

    pub async fn create_partition(&self, nodes: HashSet<NodeId>, duration: Duration) {
        let partition = NetworkPartition {
            partitioned_nodes: nodes.clone(),
            duration,
            started_at: Instant::now(),
        };
        
        self.partitions.write().await.push(partition);
        warn!("Created network partition with nodes: {:?} for {:?}", nodes, duration);
    }

    pub async fn heal_partitions(&self) {
        self.partitions.write().await.clear();
        info!("Healed all network partitions");
    }

    pub async fn send_message(&self, from: NodeId, to: NodeId, message: ProtocolMessage) -> Result<()> {
        let mut stats = self.message_stats.lock().await;
        stats.messages_sent += 1;
        drop(stats);

        // Check if nodes are partitioned
        if self.are_nodes_partitioned(from, to).await {
            debug!("Message from {} to {} dropped due to partition", from, to);
            let mut stats = self.message_stats.lock().await;
            stats.messages_dropped += 1;
            return Ok(());
        }

        let conditions = self.conditions.read().await.clone();
        
        // Simulate packet loss
        if rand::thread_rng().gen::<f64>() < conditions.packet_loss_rate {
            debug!("Message from {} to {} dropped due to packet loss", from, to);
            let mut stats = self.message_stats.lock().await;
            stats.messages_dropped += 1;
            return Ok(());
        }

        // Calculate delivery time with latency
        let latency = Duration::from_millis(
            rand::thread_rng().gen_range(
                conditions.latency_min.as_millis()..=conditions.latency_max.as_millis()
            ) as u64
        );
        
        let deliver_at = Instant::now() + latency;
        let message_size = self.estimate_message_size(&message);

        let pending = PendingMessage {
            from,
            to,
            message,
            deliver_at,
            size_bytes: message_size,
        };

        self.pending_messages.lock().await.push_back(pending);
        Ok(())
    }

    async fn are_nodes_partitioned(&self, node1: NodeId, node2: NodeId) -> bool {
        let partitions = self.partitions.read().await;
        let now = Instant::now();

        for partition in partitions.iter() {
            if now < partition.started_at + partition.duration {
                let node1_in_partition = partition.partitioned_nodes.contains(&node1);
                let node2_in_partition = partition.partitioned_nodes.contains(&node2);
                
                // Nodes are partitioned if exactly one is in the partition
                if node1_in_partition != node2_in_partition {
                    return true;
                }
            }
        }
        false
    }

    fn estimate_message_size(&self, message: &ProtocolMessage) -> usize {
        // Rough estimate of serialized message size
        // In a real implementation, we'd use the actual serializer
        let base_size = 64; // UUID + timestamps + type info
        let payload_size = match &message.message_type {
            rabia_core::messages::MessageType::Propose(propose) => {
                let batch_size = propose.batch.as_ref()
                    .map(|b| b.commands.len() * 128) // Estimate 128 bytes per command
                    .unwrap_or(0);
                32 + batch_size
            }
            rabia_core::messages::MessageType::VoteRound1(_) => 32,
            rabia_core::messages::MessageType::VoteRound2(vote) => {
                32 + vote.round1_votes.len() * 16
            }
            rabia_core::messages::MessageType::Decision(decision) => {
                let batch_size = decision.batch.as_ref()
                    .map(|b| b.commands.len() * 128)
                    .unwrap_or(0);
                32 + batch_size
            }
            rabia_core::messages::MessageType::SyncRequest(_) => 16,
            rabia_core::messages::MessageType::SyncResponse(response) => {
                let batch_size = response.pending_batches.len() * 128;
                let phase_size = response.committed_phases.len() * 32;
                64 + batch_size + phase_size
            }
            rabia_core::messages::MessageType::NewBatch(new_batch) => {
                32 + new_batch.batch.commands.len() * 128
            }
            rabia_core::messages::MessageType::HeartBeat(_) => 24,
            rabia_core::messages::MessageType::QuorumNotification(notif) => {
                16 + notif.active_nodes.len() * 16
            }
        };
        base_size + payload_size
    }

    pub async fn run_simulation(&self) {
        info!("Starting network simulation");
        let mut message_delivery_interval = tokio::time::interval(Duration::from_millis(1));
        let mut partition_cleanup_interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = message_delivery_interval.tick() => {
                    self.deliver_pending_messages().await;
                }
                
                _ = partition_cleanup_interval.tick() => {
                    self.cleanup_expired_partitions().await;
                }
                
                _ = sleep(Duration::from_millis(100)) => {
                    if *self.shutdown.lock().await {
                        break;
                    }
                }
            }
        }
        
        info!("Network simulation stopped");
    }

    async fn deliver_pending_messages(&self) {
        let now = Instant::now();
        let mut pending = self.pending_messages.lock().await;
        let nodes = self.nodes.read().await;
        
        while let Some(msg) = pending.front() {
            if msg.deliver_at <= now {
                let msg = pending.pop_front().unwrap();
                
                if let Some(target_tx) = nodes.get(&msg.to) {
                    if target_tx.send((msg.from, msg.message)).is_ok() {
                        let mut stats = self.message_stats.lock().await;
                        stats.messages_delivered += 1;
                        stats.total_latency += now.duration_since(msg.deliver_at);
                        stats.total_bytes += msg.size_bytes;
                        
                        debug!("Delivered message from {} to {}", msg.from, msg.to);
                    } else {
                        debug!("Failed to deliver message to {} (receiver dropped)", msg.to);
                    }
                } else {
                    debug!("Target node {} not found for message delivery", msg.to);
                }
            } else {
                break;
            }
        }
    }

    async fn cleanup_expired_partitions(&self) {
        let now = Instant::now();
        let mut partitions = self.partitions.write().await;
        
        partitions.retain(|partition| {
            let expired = now >= partition.started_at + partition.duration;
            if expired {
                info!("Partition expired for nodes: {:?}", partition.partitioned_nodes);
            }
            !expired
        });
    }

    pub async fn get_stats(&self) -> NetworkStats {
        (*self.message_stats.lock().await).clone()
    }

    pub async fn shutdown(&self) {
        *self.shutdown.lock().await = true;
        info!("Network simulation shutdown requested");
    }
}

impl Default for NetworkSimulator {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SimulatedNetwork {
    node_id: NodeId,
    simulator: Arc<NetworkSimulator>,
    connected_nodes: Arc<RwLock<HashSet<NodeId>>>,
    message_rx: Arc<Mutex<mpsc::UnboundedReceiver<(NodeId, ProtocolMessage)>>>,
}

impl SimulatedNetwork {
    pub async fn new(node_id: NodeId, simulator: Arc<NetworkSimulator>) -> Self {
        let message_rx = simulator.add_node(node_id).await;
        
        Self {
            node_id,
            simulator,
            connected_nodes: Arc::new(RwLock::new(HashSet::new())),
            message_rx: Arc::new(Mutex::new(message_rx)),
        }
    }

    pub async fn connect_to_nodes(&self, nodes: HashSet<NodeId>) {
        *self.connected_nodes.write().await = nodes;
    }
}

#[async_trait::async_trait]
impl NetworkTransport for SimulatedNetwork {
    async fn send_to(&self, target: NodeId, message: ProtocolMessage) -> Result<()> {
        self.simulator.send_message(self.node_id, target, message).await
    }

    async fn broadcast(&self, message: ProtocolMessage, exclude: Option<NodeId>) -> Result<()> {
        let connected = self.connected_nodes.read().await;
        
        for &node_id in connected.iter() {
            if Some(node_id) != exclude && node_id != self.node_id {
                self.simulator.send_message(self.node_id, node_id, message.clone()).await?;
            }
        }
        Ok(())
    }

    async fn receive(&mut self) -> Result<(NodeId, ProtocolMessage)> {
        let mut rx = self.message_rx.lock().await;
        
        match rx.recv().await {
            Some((from, message)) => Ok((from, message)),
            None => Err(RabiaError::network("Message channel closed")),
        }
    }

    async fn get_connected_nodes(&self) -> Result<HashSet<NodeId>> {
        Ok(self.connected_nodes.read().await.clone())
    }

    async fn is_connected(&self, node_id: NodeId) -> Result<bool> {
        Ok(self.connected_nodes.read().await.contains(&node_id))
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected_nodes.write().await.clear();
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<()> {
        // In simulation, reconnection would involve re-adding to simulator
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_message_delivery() {
        let simulator = Arc::new(NetworkSimulator::new());
        
        let node1 = NodeId::new();
        let node2 = NodeId::new();
        
        let mut rx1 = simulator.add_node(node1).await;
        let _rx2 = simulator.add_node(node2).await;
        
        // Start simulation
        let sim_handle = {
            let sim = simulator.clone();
            tokio::spawn(async move {
                sim.run_simulation().await;
            })
        };
        
        // Send a message
        let message = rabia_core::messages::ProtocolMessage::new(
            node2,
            Some(node1),
            rabia_core::messages::MessageType::HeartBeat(
                rabia_core::messages::HeartBeatMessage {
                    current_phase: rabia_core::PhaseId::new(1),
                    last_committed_phase: rabia_core::PhaseId::new(0),
                    active: true,
                }
            )
        );
        
        simulator.send_message(node2, node1, message).await.unwrap();
        
        // Wait for delivery
        sleep(Duration::from_millis(50)).await;
        
        // Check message was received
        let received = tokio::time::timeout(Duration::from_millis(100), rx1.recv()).await;
        assert!(received.is_ok());
        
        let stats = simulator.get_stats().await;
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.messages_delivered, 1);
        
        simulator.shutdown().await;
        sim_handle.abort();
    }

    #[tokio::test]
    async fn test_network_partition() {
        let simulator = Arc::new(NetworkSimulator::new());
        
        let node1 = NodeId::new();
        let node2 = NodeId::new();
        
        let mut rx1 = simulator.add_node(node1).await;
        let _rx2 = simulator.add_node(node2).await;
        
        // Create partition
        let mut partitioned_nodes = HashSet::new();
        partitioned_nodes.insert(node1);
        simulator.create_partition(partitioned_nodes, Duration::from_millis(100)).await;
        
        // Start simulation
        let sim_handle = {
            let sim = simulator.clone();
            tokio::spawn(async move {
                sim.run_simulation().await;
            })
        };
        
        // Send message during partition
        let message = rabia_core::messages::ProtocolMessage::new(
            node2,
            Some(node1),
            rabia_core::messages::MessageType::HeartBeat(
                rabia_core::messages::HeartBeatMessage {
                    current_phase: rabia_core::PhaseId::new(1),
                    last_committed_phase: rabia_core::PhaseId::new(0),
                    active: true,
                }
            )
        );
        
        simulator.send_message(node2, node1, message).await.unwrap();
        
        // Wait and check message was dropped
        sleep(Duration::from_millis(50)).await;
        
        let received = tokio::time::timeout(Duration::from_millis(50), rx1.recv()).await;
        assert!(received.is_err()); // Should timeout
        
        let stats = simulator.get_stats().await;
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.messages_dropped, 1);
        
        simulator.shutdown().await;
        sim_handle.abort();
    }
}