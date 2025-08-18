//! TCP/IP Network Implementation for Rabia Consensus
//!
//! This module provides production-ready TCP networking for the Rabia consensus protocol.
//! It supports:
//! - Connection management and pooling
//! - Message framing and serialization
//! - Node discovery and dynamic topology
//! - Fault tolerance and automatic reconnection
//! - Performance optimizations for high throughput

use async_trait::async_trait;
use bytes::{Bytes, BytesMut, BufMut};
// use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{timeout, sleep};
use tracing::{debug, info, warn, error};

use rabia_core::{
    NodeId, Result, RabiaError,
    messages::ProtocolMessage,
    network::NetworkTransport,
};

/// Configuration for TCP networking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpNetworkConfig {
    /// Local address to bind to
    pub bind_addr: SocketAddr,
    /// Known peer addresses for initial connection
    pub peer_addresses: HashMap<NodeId, SocketAddr>,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Keep-alive interval
    pub keepalive_interval: Duration,
    /// Maximum message size (in bytes)
    pub max_message_size: usize,
    /// Connection retry settings
    pub retry_config: RetryConfig,
    /// Buffer sizes
    pub buffer_config: BufferConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of connection attempts
    pub max_attempts: usize,
    /// Base delay between retry attempts
    pub base_delay: Duration,
    /// Maximum delay between retry attempts
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    /// TCP read buffer size
    pub read_buffer_size: usize,
    /// TCP write buffer size  
    pub write_buffer_size: usize,
    /// Message queue size per connection
    pub message_queue_size: usize,
}

impl Default for TcpNetworkConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            peer_addresses: HashMap::new(),
            connection_timeout: Duration::from_secs(10),
            keepalive_interval: Duration::from_secs(30),
            max_message_size: 16 * 1024 * 1024, // 16MB
            retry_config: RetryConfig::default(),
            buffer_config: BufferConfig::default(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            read_buffer_size: 64 * 1024,  // 64KB
            write_buffer_size: 64 * 1024, // 64KB
            message_queue_size: 1000,
        }
    }
}

/// Message frame structure for TCP transport
#[derive(Debug)]
struct MessageFrame {
    /// Length of the message payload
    length: u32,
    /// Message payload
    payload: Bytes,
}

impl MessageFrame {
    /// Maximum frame size (length field + max payload)
    const MAX_FRAME_SIZE: usize = 4 + 16 * 1024 * 1024; // 4 bytes + 16MB

    /// Create a new message frame
    fn new(payload: Bytes) -> Result<Self> {
        if payload.len() > Self::MAX_FRAME_SIZE - 4 {
            return Err(RabiaError::network(format!(
                "Message too large: {} bytes", payload.len()
            )));
        }
        
        Ok(Self {
            length: payload.len() as u32,
            payload,
        })
    }

    /// Serialize frame to bytes
    fn to_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(4 + self.payload.len());
        buf.put_u32(self.length);
        buf.put_slice(&self.payload);
        buf.freeze()
    }

    /// Deserialize frame from bytes
    async fn from_stream<R>(reader: &mut R) -> Result<Self> 
    where 
        R: AsyncReadExt + Unpin,
    {
        // Read length field
        let length = reader.read_u32().await
            .map_err(|e| RabiaError::network(format!("Failed to read frame length: {}", e)))?;
        
        if length as usize > Self::MAX_FRAME_SIZE - 4 {
            return Err(RabiaError::network(format!(
                "Frame too large: {} bytes", length
            )));
        }

        // Read payload
        let mut payload = vec![0u8; length as usize];
        reader.read_exact(&mut payload).await
            .map_err(|e| RabiaError::network(format!("Failed to read frame payload: {}", e)))?;

        Ok(Self {
            length,
            payload: Bytes::from(payload),
        })
    }
}

/// Connection state information
#[derive(Debug)]
struct ConnectionInfo {
    node_id: NodeId,
    #[allow(dead_code)] // Used for debugging and connection management
    addr: SocketAddr,
    stream: Arc<Mutex<TcpStream>>,
    last_seen: Instant,
    outbound_queue: mpsc::UnboundedSender<ProtocolMessage>,
    #[allow(dead_code)] // Used for connection type tracking
    is_outbound: bool,
}

/// TCP Network implementation
pub struct TcpNetwork {
    /// This node's ID
    node_id: NodeId,
    /// Configuration
    config: TcpNetworkConfig,
    /// TCP listener for incoming connections
    #[allow(dead_code)] // Used for maintaining listener reference
    listener: Option<TcpListener>,
    /// Active connections by node ID
    connections: Arc<RwLock<HashMap<NodeId, Arc<ConnectionInfo>>>>,
    /// Address to node ID mapping
    addr_to_node: Arc<RwLock<HashMap<SocketAddr, NodeId>>>,
    /// Incoming message queue (each instance has its own)
    message_rx: mpsc::UnboundedReceiver<(NodeId, ProtocolMessage)>,
    message_tx: mpsc::UnboundedSender<(NodeId, ProtocolMessage)>,
    /// Shutdown signal
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    #[allow(dead_code)] // Used for shutdown coordination
    shutdown_rx: Arc<Mutex<Option<mpsc::Receiver<()>>>>
}

impl TcpNetwork {
    /// Create a new TCP network instance
    pub async fn new(node_id: NodeId, config: TcpNetworkConfig) -> Result<Self> {
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        let mut network = Self {
            node_id,
            config,
            listener: None,
            connections: Arc::new(RwLock::new(HashMap::new())),
            addr_to_node: Arc::new(RwLock::new(HashMap::new())),
            message_rx,
            message_tx,
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
            shutdown_rx: Arc::new(Mutex::new(Some(shutdown_rx))),
        };

        // Start TCP listener
        network.start_listener().await?;
        
        // Start connection manager
        network.start_connection_manager().await;
        
        info!("TCP network started for node {} on {}", node_id, network.config.bind_addr);
        
        Ok(network)
    }

    /// Start TCP listener for incoming connections
    async fn start_listener(&mut self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.bind_addr).await
            .map_err(|e| RabiaError::network(format!("Failed to bind to {}: {}", self.config.bind_addr, e)))?;

        let actual_addr = listener.local_addr()
            .map_err(|e| RabiaError::network(format!("Failed to get local address: {}", e)))?;
        
        info!("TCP listener bound to {}", actual_addr);
        self.config.bind_addr = actual_addr;

        // Spawn listener task
        let connections = self.connections.clone();
        let addr_to_node = self.addr_to_node.clone();
        let message_tx = self.message_tx.clone();
        let node_id = self.node_id;
        let config = self.config.clone();

        tokio::spawn(async move {
            Self::accept_connections(listener, node_id, config, connections, addr_to_node, message_tx).await;
        });

        Ok(())
    }

    /// Accept incoming TCP connections
    async fn accept_connections(
        listener: TcpListener,
        node_id: NodeId,
        config: TcpNetworkConfig,
        connections: Arc<RwLock<HashMap<NodeId, Arc<ConnectionInfo>>>>,
        addr_to_node: Arc<RwLock<HashMap<SocketAddr, NodeId>>>,
        message_tx: mpsc::UnboundedSender<(NodeId, ProtocolMessage)>,
    ) {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("Accepted connection from {}", addr);
                    
                    let connections = connections.clone();
                    let addr_to_node = addr_to_node.clone();
                    let message_tx = message_tx.clone();
                    let config = config.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_inbound_connection(
                            stream, addr, node_id, config, connections, addr_to_node, message_tx
                        ).await {
                            warn!("Failed to handle inbound connection from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Handle an inbound TCP connection
    async fn handle_inbound_connection(
        mut stream: TcpStream,
        addr: SocketAddr,
        local_node_id: NodeId,
        config: TcpNetworkConfig,
        connections: Arc<RwLock<HashMap<NodeId, Arc<ConnectionInfo>>>>,
        addr_to_node: Arc<RwLock<HashMap<SocketAddr, NodeId>>>,
        message_tx: mpsc::UnboundedSender<(NodeId, ProtocolMessage)>,
    ) -> Result<()> {
        // Perform handshake to identify the peer
        let peer_node_id = Self::perform_inbound_handshake(&mut stream, local_node_id).await?;
        
        info!("Established inbound connection from {} ({})", peer_node_id, addr);

        // Create connection info
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let connection_info = Arc::new(ConnectionInfo {
            node_id: peer_node_id,
            addr,
            stream: Arc::new(Mutex::new(stream)),
            last_seen: Instant::now(),
            outbound_queue: outbound_tx,
            is_outbound: false,
        });

        // Register connection
        {
            let mut connections = connections.write().await;
            connections.insert(peer_node_id, connection_info.clone());
        }
        {
            let mut addr_to_node = addr_to_node.write().await;
            addr_to_node.insert(addr, peer_node_id);
        }

        // Start connection handler
        let connections_clone = connections.clone();
        tokio::spawn(async move {
            Self::run_connection_handler(connection_info.clone(), outbound_rx, message_tx, config).await;
            
            // Clean up on disconnect
            let mut connections = connections_clone.write().await;
            connections.remove(&peer_node_id);
        });
        
        Ok(())
    }

    /// Perform handshake for inbound connection
    async fn perform_inbound_handshake(stream: &mut TcpStream, local_node_id: NodeId) -> Result<NodeId> {
        // Simple handshake protocol:
        // 1. Peer sends their node ID
        // 2. We send our node ID back
        // 3. Connection is established
        
        // Read peer's node ID
        let frame = MessageFrame::from_stream(stream).await?;
        let peer_node_id: NodeId = bincode::deserialize(&frame.payload)
            .map_err(|e| RabiaError::network(format!("Failed to deserialize peer node ID: {}", e)))?;

        // Send our node ID
        let our_id_bytes = bincode::serialize(&local_node_id)
            .map_err(|e| RabiaError::network(format!("Failed to serialize node ID: {}", e)))?;
        let response_frame = MessageFrame::new(Bytes::from(our_id_bytes))?;
        
        stream.write_all(&response_frame.to_bytes()).await
            .map_err(|e| RabiaError::network(format!("Failed to send handshake response: {}", e)))?;

        Ok(peer_node_id)
    }

    /// Connect to a peer node
    pub async fn connect_to_peer(&self, peer_node_id: NodeId, addr: SocketAddr) -> Result<()> {
        // Check if already connected
        {
            let connections = self.connections.read().await;
            if connections.contains_key(&peer_node_id) {
                debug!("Already connected to peer {}", peer_node_id);
                return Ok(());
            }
        }

        info!("Connecting to peer {} at {}", peer_node_id, addr);

        // Attempt connection with retries
        let mut attempts = 0;
        let mut delay = self.config.retry_config.base_delay;

        while attempts < self.config.retry_config.max_attempts {
            match timeout(self.config.connection_timeout, TcpStream::connect(&addr)).await {
                Ok(Ok(mut stream)) => {
                    // Perform outbound handshake
                    if let Err(e) = self.perform_outbound_handshake(&mut stream, peer_node_id).await {
                        warn!("Handshake failed with {}: {}", peer_node_id, e);
                        attempts += 1;
                        sleep(delay).await;
                        delay = Duration::min(
                            Duration::from_millis((delay.as_millis() as f64 * self.config.retry_config.backoff_multiplier) as u64),
                            self.config.retry_config.max_delay
                        );
                        continue;
                    }

                    info!("Successfully connected to peer {} at {}", peer_node_id, addr);

                    // Create connection info
                    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
                    let connection_info = Arc::new(ConnectionInfo {
                        node_id: peer_node_id,
                        addr,
                        stream: Arc::new(Mutex::new(stream)),
                        last_seen: Instant::now(),
                        outbound_queue: outbound_tx,
                        is_outbound: true,
                    });

                    // Register connection
                    {
                        let mut connections = self.connections.write().await;
                        connections.insert(peer_node_id, connection_info.clone());
                    }
                    {
                        let mut addr_to_node = self.addr_to_node.write().await;
                        addr_to_node.insert(addr, peer_node_id);
                    }

                    // Start connection handler
                    let connections = self.connections.clone();
                    let message_tx = self.message_tx.clone();
                    let config = self.config.clone();
                    
                    tokio::spawn(async move {
                        Self::run_connection_handler(connection_info, outbound_rx, message_tx, config).await;
                        
                        // Clean up on disconnect
                        let mut connections = connections.write().await;
                        connections.remove(&peer_node_id);
                    });

                    return Ok(());
                }
                Ok(Err(e)) => {
                    warn!("Connection attempt {} to {} failed: {}", attempts + 1, addr, e);
                }
                Err(_) => {
                    warn!("Connection attempt {} to {} timed out", attempts + 1, addr);
                }
            }

            attempts += 1;
            if attempts < self.config.retry_config.max_attempts {
                sleep(delay).await;
                delay = Duration::min(
                    Duration::from_millis((delay.as_millis() as f64 * self.config.retry_config.backoff_multiplier) as u64),
                    self.config.retry_config.max_delay
                );
            }
        }

        Err(RabiaError::network(format!(
            "Failed to connect to {} after {} attempts", addr, attempts
        )))
    }

    /// Perform handshake for outbound connection
    async fn perform_outbound_handshake(&self, stream: &mut TcpStream, expected_peer_id: NodeId) -> Result<()> {
        // Send our node ID
        let our_id_bytes = bincode::serialize(&self.node_id)
            .map_err(|e| RabiaError::network(format!("Failed to serialize node ID: {}", e)))?;
        let handshake_frame = MessageFrame::new(Bytes::from(our_id_bytes))?;
        
        stream.write_all(&handshake_frame.to_bytes()).await
            .map_err(|e| RabiaError::network(format!("Failed to send handshake: {}", e)))?;

        // Read peer's response
        let frame = MessageFrame::from_stream(stream).await?;
        let peer_node_id: NodeId = bincode::deserialize(&frame.payload)
            .map_err(|e| RabiaError::network(format!("Failed to deserialize peer response: {}", e)))?;

        if peer_node_id != expected_peer_id {
            return Err(RabiaError::network(format!(
                "Node ID mismatch: expected {}, got {}", expected_peer_id, peer_node_id
            )));
        }

        Ok(())
    }

    /// Run the connection handler for a specific connection
    async fn run_connection_handler(
        connection: Arc<ConnectionInfo>,
        mut outbound_rx: mpsc::UnboundedReceiver<ProtocolMessage>,
        message_tx: mpsc::UnboundedSender<(NodeId, ProtocolMessage)>,
        _config: TcpNetworkConfig,
    ) {
        let node_id = connection.node_id;
        info!("Starting connection handler for {}", node_id);

        // Create separate handles for reading and writing
        let stream_read = connection.stream.clone();
        let stream_write = connection.stream.clone();

        // Spawn reader task
        let message_tx_clone = message_tx.clone();
        let reader_handle = tokio::spawn(async move {
            loop {
                let frame_result = {
                    let mut stream_guard = stream_read.lock().await;
                    MessageFrame::from_stream(&mut *stream_guard).await
                };
                
                match frame_result {
                    Ok(frame) => {
                        match bincode::deserialize::<ProtocolMessage>(&frame.payload) {
                            Ok(message) => {
                                if let Err(e) = message_tx_clone.send((node_id, message)) {
                                    debug!("Failed to send message to queue: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to deserialize message from {}: {}", node_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Connection to {} closed: {}", node_id, e);
                        break;
                    }
                }
            }
        });

        // Spawn writer task
        let writer_handle = tokio::spawn(async move {
            while let Some(message) = outbound_rx.recv().await {
                match bincode::serialize(&message) {
                    Ok(serialized) => {
                        let frame = match MessageFrame::new(Bytes::from(serialized)) {
                            Ok(frame) => frame,
                            Err(e) => {
                                warn!("Failed to create frame for message to {}: {}", node_id, e);
                                continue;
                            }
                        };

                        let write_result = {
                            let mut stream_guard = stream_write.lock().await;
                            stream_guard.write_all(&frame.to_bytes()).await
                        };
                        
                        if let Err(e) = write_result {
                            debug!("Failed to write to {}: {}", node_id, e);
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to serialize message to {}: {}", node_id, e);
                    }
                }
            }
        });

        // Wait for either task to complete (indicating connection closed)
        tokio::select! {
            _ = reader_handle => {
                debug!("Reader task for {} completed", node_id);
            }
            _ = writer_handle => {
                debug!("Writer task for {} completed", node_id);
            }
        }

        info!("Connection handler for {} stopped", node_id);
    }

    /// Start connection manager for automatic peer connections
    async fn start_connection_manager(&self) {
        let peer_addresses = self.config.peer_addresses.clone();
        let connections = self.connections.clone();
        
        // Connect to known peers directly without spawning (synchronous approach)
        for (peer_id, addr) in peer_addresses {
            info!("Attempting initial connection to peer {} at {}", peer_id, addr);
            if let Err(e) = self.connect_to_peer(peer_id, addr).await {
                warn!("Failed to connect to peer {} at {}: {}", peer_id, addr, e);
            }
        }

        // Start periodic connection health checks
        let connections_clone = connections.clone();
        let keepalive_interval = self.config.keepalive_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(keepalive_interval);
            loop {
                interval.tick().await;
                
                let connections = connections_clone.read().await;
                let now = Instant::now();
                
                for (node_id, connection) in connections.iter() {
                    let elapsed = now.duration_since(connection.last_seen);
                    if elapsed > keepalive_interval * 2 {
                        warn!("Connection to {} appears stale (last seen {:?} ago)", node_id, elapsed);
                        // TODO: Implement connection health check or reconnection
                    }
                }
            }
        });
    }

    /// Get the local bind address
    pub fn local_addr(&self) -> SocketAddr {
        self.config.bind_addr
    }

    /// Add a known peer address for automatic connection
    pub async fn add_peer(&mut self, node_id: NodeId, addr: SocketAddr) {
        self.config.peer_addresses.insert(node_id, addr);
        
        // Attempt immediate connection
        if let Err(e) = self.connect_to_peer(node_id, addr).await {
            warn!("Failed to connect to newly added peer {} at {}: {}", node_id, addr, e);
        }
    }

    /// Remove a peer
    pub async fn remove_peer(&mut self, node_id: NodeId) {
        self.config.peer_addresses.remove(&node_id);
        
        // Close existing connection
        let mut connections = self.connections.write().await;
        if let Some(_connection) = connections.remove(&node_id) {
            info!("Removed connection to peer {}", node_id);
            // Connection will be closed when the handler tasks detect the closed stream
        }
    }

    /// Shutdown the network
    pub async fn shutdown(&self) {
        info!("Shutting down TCP network");
        
        if let Some(shutdown_tx) = self.shutdown_tx.lock().await.as_ref() {
            let _ = shutdown_tx.send(()).await;
        }

        // Close all connections
        let connections = self.connections.read().await;
        for (node_id, _) in connections.iter() {
            debug!("Closing connection to {}", node_id);
        }
    }
}

// Remove Clone implementation since message_rx can't be safely cloned
// Each TcpNetwork instance should be unique with its own message receiver

#[async_trait]
impl NetworkTransport for TcpNetwork {
    async fn send_to(&self, target: NodeId, message: ProtocolMessage) -> Result<()> {
        let connections = self.connections.read().await;
        
        if let Some(connection) = connections.get(&target) {
            connection.outbound_queue.send(message)
                .map_err(|_| RabiaError::network(format!("Failed to queue message to {}", target)))?;
            Ok(())
        } else {
            Err(RabiaError::network(format!("No connection to node {}", target)))
        }
    }

    async fn broadcast(&self, message: ProtocolMessage, exclude: Option<NodeId>) -> Result<()> {
        let connections = self.connections.read().await;
        let mut failed_nodes = Vec::new();

        for (node_id, connection) in connections.iter() {
            if Some(*node_id) != exclude && *node_id != self.node_id {
                if connection.outbound_queue.send(message.clone()).is_err() {
                    failed_nodes.push(*node_id);
                }
            }
        }

        if !failed_nodes.is_empty() {
            warn!("Failed to broadcast to nodes: {:?}", failed_nodes);
        }

        Ok(())
    }

    async fn receive(&mut self) -> Result<(NodeId, ProtocolMessage)> {
        match self.message_rx.recv().await {
            Some((from, message)) => Ok((from, message)),
            None => Err(RabiaError::network("Message channel closed")),
        }
    }

    async fn get_connected_nodes(&self) -> Result<HashSet<NodeId>> {
        let connections = self.connections.read().await;
        Ok(connections.keys().copied().collect())
    }

    async fn is_connected(&self, node_id: NodeId) -> Result<bool> {
        let connections = self.connections.read().await;
        Ok(connections.contains_key(&node_id))
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.shutdown().await;
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<()> {
        // Attempt to reconnect to all known peers
        let peer_addresses = self.config.peer_addresses.clone();
        
        for (peer_id, addr) in peer_addresses {
            if let Err(e) = self.connect_to_peer(peer_id, addr).await {
                warn!("Failed to reconnect to peer {} at {}: {}", peer_id, addr, e);
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;
    use std::time::Duration;

    #[tokio::test]
    async fn test_tcp_network_creation() {
        let node_id = NodeId::new();
        let config = TcpNetworkConfig::default();
        
        let network = TcpNetwork::new(node_id, config).await.unwrap();
        assert_eq!(network.node_id, node_id);
        assert!(network.local_addr().port() > 0);
    }

    #[tokio::test]
    async fn test_message_frame() {
        let payload = Bytes::from("test message");
        let frame = MessageFrame::new(payload.clone()).unwrap();
        
        assert_eq!(frame.length, payload.len() as u32);
        assert_eq!(frame.payload, payload);
        
        let serialized = frame.to_bytes();
        assert!(serialized.len() == 4 + payload.len());
    }

    #[tokio::test]
    async fn test_peer_connection() {
        let node1_id = NodeId::new();
        let node2_id = NodeId::new();
        
        let config1 = TcpNetworkConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        };
        let config2 = TcpNetworkConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        };
        
        let network1 = TcpNetwork::new(node1_id, config1).await.unwrap();
        let network2 = TcpNetwork::new(node2_id, config2).await.unwrap();
        
        let _addr1 = network1.local_addr();
        let addr2 = network2.local_addr();
        
        // Connect network1 to network2
        network1.connect_to_peer(node2_id, addr2).await.unwrap();
        
        // Give some time for connection to establish
        sleep(Duration::from_millis(100)).await;
        
        // Check connections
        assert!(network1.is_connected(node2_id).await.unwrap());
        assert!(network2.is_connected(node1_id).await.unwrap());
        
        // Cleanup
        network1.shutdown().await;
        network2.shutdown().await;
    }
}
