use async_trait::async_trait;
use rabia_core::{
    messages::ProtocolMessage, network::NetworkTransport, NodeId, RabiaError, Result,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug)]
pub struct InMemoryNetwork {
    node_id: NodeId,
    message_queue: Arc<Mutex<VecDeque<(NodeId, ProtocolMessage)>>>,
    connected_nodes: Arc<Mutex<HashSet<NodeId>>>,
    #[allow(clippy::type_complexity)]
    network_bus: Arc<Mutex<Option<mpsc::UnboundedSender<(NodeId, NodeId, ProtocolMessage)>>>>,
}

impl InMemoryNetwork {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
            connected_nodes: Arc::new(Mutex::new(HashSet::new())),
            network_bus: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn connect_to_bus(
        &self,
        bus: mpsc::UnboundedSender<(NodeId, NodeId, ProtocolMessage)>,
    ) {
        let mut network_bus = self.network_bus.lock().await;
        *network_bus = Some(bus);
    }

    pub async fn deliver_message(&self, from: NodeId, message: ProtocolMessage) {
        let mut queue = self.message_queue.lock().await;
        queue.push_back((from, message));
    }

    pub async fn set_connected_nodes(&self, nodes: HashSet<NodeId>) {
        let mut connected = self.connected_nodes.lock().await;
        *connected = nodes;
    }
}

#[async_trait]
impl NetworkTransport for InMemoryNetwork {
    async fn send_to(&self, target: NodeId, message: ProtocolMessage) -> Result<()> {
        let bus = self.network_bus.lock().await;
        if let Some(bus) = bus.as_ref() {
            bus.send((self.node_id, target, message))
                .map_err(|_| RabiaError::network("Failed to send message to bus"))?;
        }
        Ok(())
    }

    async fn broadcast(&self, message: ProtocolMessage, exclude: Option<NodeId>) -> Result<()> {
        let connected = self.connected_nodes.lock().await;
        let bus = self.network_bus.lock().await;

        if let Some(bus) = bus.as_ref() {
            for &node_id in connected.iter() {
                if Some(node_id) != exclude && node_id != self.node_id {
                    bus.send((self.node_id, node_id, message.clone()))
                        .map_err(|_| RabiaError::network("Failed to broadcast message"))?;
                }
            }
        }
        Ok(())
    }

    async fn receive(&mut self) -> Result<(NodeId, ProtocolMessage)> {
        let mut queue = self.message_queue.lock().await;
        if let Some((from, message)) = queue.pop_front() {
            Ok((from, message))
        } else {
            // In a real implementation, this would block waiting for messages
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Err(RabiaError::network("No messages available"))
        }
    }

    async fn get_connected_nodes(&self) -> Result<HashSet<NodeId>> {
        let connected = self.connected_nodes.lock().await;
        Ok(connected.clone())
    }

    async fn is_connected(&self, node_id: NodeId) -> Result<bool> {
        let connected = self.connected_nodes.lock().await;
        Ok(connected.contains(&node_id))
    }

    async fn disconnect(&mut self) -> Result<()> {
        let mut connected = self.connected_nodes.lock().await;
        connected.clear();
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<()> {
        // In a real implementation, this would attempt to reconnect to the network
        Ok(())
    }
}

pub struct InMemoryNetworkSimulator {
    nodes: HashMap<NodeId, mpsc::UnboundedSender<(NodeId, ProtocolMessage)>>,
    message_bus: mpsc::UnboundedReceiver<(NodeId, NodeId, ProtocolMessage)>,
}

impl InMemoryNetworkSimulator {
    pub fn new() -> (
        Self,
        mpsc::UnboundedSender<(NodeId, NodeId, ProtocolMessage)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                nodes: HashMap::new(),
                message_bus: rx,
            },
            tx,
        )
    }

    pub fn add_node(
        &mut self,
        node_id: NodeId,
        sender: mpsc::UnboundedSender<(NodeId, ProtocolMessage)>,
    ) {
        self.nodes.insert(node_id, sender);
    }

    pub async fn run(&mut self) {
        while let Some((from, to, message)) = self.message_bus.recv().await {
            if let Some(target_sender) = self.nodes.get(&to) {
                let _ = target_sender.send((from, message));
            }
        }
    }
}
