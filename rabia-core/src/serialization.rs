use crate::{
    memory_pool::{get_pooled_buffer, PooledBuffer},
    messages::ProtocolMessage,
    RabiaError, Result,
};
use serde::{Deserialize, Serialize};

/// Trait for efficient message serialization
pub trait MessageSerializer {
    /// Serialize a message to bytes
    fn serialize<T>(&self, data: &T) -> Result<Vec<u8>>
    where
        T: Serialize;

    /// Deserialize bytes to a message
    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>;
}

/// JSON serializer (existing implementation)
#[derive(Default, Clone)]
pub struct JsonSerializer;

impl MessageSerializer for JsonSerializer {
    fn serialize<T>(&self, data: &T) -> Result<Vec<u8>>
    where
        T: Serialize,
    {
        serde_json::to_vec(data)
            .map_err(|e| RabiaError::serialization(format!("JSON serialization failed: {}", e)))
    }

    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(bytes)
            .map_err(|e| RabiaError::serialization(format!("JSON deserialization failed: {}", e)))
    }
}

/// Binary serializer using bincode (optimized implementation)
#[derive(Default, Clone)]
pub struct BinarySerializer;

impl MessageSerializer for BinarySerializer {
    fn serialize<T>(&self, data: &T) -> Result<Vec<u8>>
    where
        T: Serialize,
    {
        bincode::serialize(data)
            .map_err(|e| RabiaError::serialization(format!("Binary serialization failed: {}", e)))
    }

    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        bincode::deserialize(bytes)
            .map_err(|e| RabiaError::serialization(format!("Binary deserialization failed: {}", e)))
    }
}

/// Enum-based serializer that avoids trait object issues
#[derive(Clone)]
pub enum Serializer {
    Json(JsonSerializer),
    Binary(BinarySerializer),
}

impl Default for Serializer {
    fn default() -> Self {
        Self::Binary(BinarySerializer)
    }
}

impl MessageSerializer for Serializer {
    fn serialize<T>(&self, data: &T) -> Result<Vec<u8>>
    where
        T: Serialize,
    {
        match self {
            Self::Json(s) => s.serialize(data),
            Self::Binary(s) => s.serialize(data),
        }
    }

    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        match self {
            Self::Json(s) => s.deserialize(bytes),
            Self::Binary(s) => s.deserialize(bytes),
        }
    }
}

/// Configuration for serialization
#[derive(Debug, Clone)]
pub struct SerializationConfig {
    pub use_binary: bool,
    pub compression_threshold: usize,
}

impl Default for SerializationConfig {
    fn default() -> Self {
        Self {
            use_binary: true,            // Default to binary for performance
            compression_threshold: 1024, // Compress only large messages
        }
    }
}

/// Factory for creating serializers based on configuration
pub struct SerializerFactory;

impl SerializerFactory {
    pub fn create(config: &SerializationConfig) -> Serializer {
        if config.use_binary {
            Serializer::Binary(BinarySerializer)
        } else {
            Serializer::Json(JsonSerializer)
        }
    }
}

/// Convenience functions for common serialization tasks
impl Serializer {
    /// Create a new JSON serializer
    pub fn json() -> Self {
        Self::Json(JsonSerializer)
    }

    /// Create a new binary serializer  
    pub fn binary() -> Self {
        Self::Binary(BinarySerializer)
    }

    /// Serialize a protocol message
    pub fn serialize_message(&self, message: &ProtocolMessage) -> Result<Vec<u8>> {
        self.serialize(message)
    }

    /// Deserialize a protocol message
    pub fn deserialize_message(&self, bytes: &[u8]) -> Result<ProtocolMessage> {
        self.deserialize(bytes)
    }

    /// Serialize a protocol message using pooled buffer (zero-allocation)
    pub fn serialize_message_pooled(&self, message: &ProtocolMessage) -> Result<PooledBuffer> {
        // Estimate size based on message type and content
        let estimated_size = estimate_message_size(message);
        let mut buffer = get_pooled_buffer(estimated_size);

        match self {
            Self::Json(serializer) => {
                let temp = serializer.serialize(message)?;
                buffer.buffer_mut().extend_from_slice(&temp);
            }
            Self::Binary(serializer) => {
                let temp = serializer.serialize(message)?;
                buffer.buffer_mut().extend_from_slice(&temp);
            }
        }

        Ok(buffer)
    }
}

/// Estimate the serialized size of a protocol message for buffer allocation
fn estimate_message_size(message: &ProtocolMessage) -> usize {
    use crate::messages::MessageType;

    let base_size = 128; // Base message overhead (IDs, timestamps, etc.)

    let payload_size = match &message.message_type {
        MessageType::Propose(propose) => {
            let batch_size = propose
                .batch
                .as_ref()
                .map(|b| b.commands.len() * 64) // Estimate 64 bytes per command
                .unwrap_or(0);
            64 + batch_size
        }
        MessageType::VoteRound1(_) => 32,
        MessageType::VoteRound2(vote) => 32 + vote.round1_votes.len() * 16,
        MessageType::Decision(decision) => {
            let batch_size = decision
                .batch
                .as_ref()
                .map(|b| b.commands.len() * 64)
                .unwrap_or(0);
            64 + batch_size
        }
        MessageType::SyncRequest(_) => 16,
        MessageType::SyncResponse(response) => {
            let batch_size = response.pending_batches.len() * 64;
            let phase_size = response.committed_phases.len() * 16;
            64 + batch_size + phase_size
        }
        MessageType::NewBatch(new_batch) => 32 + new_batch.batch.commands.len() * 64,
        MessageType::HeartBeat(_) => 24,
        MessageType::QuorumNotification(notif) => 16 + notif.active_nodes.len() * 16,
    };

    base_size + payload_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::*;
    use crate::{Command, CommandBatch, NodeId, PhaseId, StateValue};

    fn create_test_message() -> ProtocolMessage {
        ProtocolMessage::new(
            NodeId::new(),
            Some(NodeId::new()),
            MessageType::Propose(ProposeMessage {
                phase_id: PhaseId::new(1),
                batch_id: crate::BatchId::new(),
                value: StateValue::V1,
                batch: Some(CommandBatch::new(vec![
                    Command::new("SET key1 value1"),
                    Command::new("SET key2 value2"),
                    Command::new("GET key1"),
                ])),
            }),
        )
    }

    #[test]
    fn test_json_serialization() {
        let serializer = Serializer::json();
        let message = create_test_message();

        let serialized = serializer.serialize(&message).unwrap();
        let deserialized: ProtocolMessage = serializer.deserialize(&serialized).unwrap();

        assert_eq!(message.from, deserialized.from);
        assert_eq!(message.to, deserialized.to);
    }

    #[test]
    fn test_binary_serialization() {
        let serializer = Serializer::binary();
        let message = create_test_message();

        let serialized = serializer.serialize(&message).unwrap();
        let deserialized: ProtocolMessage = serializer.deserialize(&serialized).unwrap();

        assert_eq!(message.from, deserialized.from);
        assert_eq!(message.to, deserialized.to);
    }

    #[test]
    fn test_binary_vs_json_size() {
        let message = create_test_message();

        let json_serializer = Serializer::json();
        let binary_serializer = Serializer::binary();

        let json_bytes = json_serializer.serialize(&message).unwrap();
        let binary_bytes = binary_serializer.serialize(&message).unwrap();

        // Binary should be smaller than JSON
        assert!(binary_bytes.len() < json_bytes.len());
        println!("JSON size: {} bytes", json_bytes.len());
        println!("Binary size: {} bytes", binary_bytes.len());
        println!(
            "Size reduction: {:.1}%",
            (json_bytes.len() - binary_bytes.len()) as f64 / json_bytes.len() as f64 * 100.0
        );
    }

    #[test]
    fn test_serializer_factory() {
        let config = SerializationConfig::default();
        let serializer = SerializerFactory::create(&config);

        let message = create_test_message();
        let serialized = serializer.serialize(&message).unwrap();
        let deserialized: ProtocolMessage = serializer.deserialize(&serialized).unwrap();

        assert_eq!(message.from, deserialized.from);
    }

    #[test]
    fn test_protocol_message_convenience_methods() {
        let serializer = Serializer::binary();
        let message = create_test_message();

        let serialized = serializer.serialize_message(&message).unwrap();
        let deserialized = serializer.deserialize_message(&serialized).unwrap();

        assert_eq!(message.from, deserialized.from);
        assert_eq!(message.to, deserialized.to);
    }

    #[test]
    fn test_pooled_serialization() {
        let serializer = Serializer::binary();
        let message = create_test_message();

        // Test pooled serialization
        let pooled_buffer = serializer.serialize_message_pooled(&message).unwrap();
        let regular_serialized = serializer.serialize_message(&message).unwrap();

        // Should produce same result
        assert_eq!(pooled_buffer.as_slice(), regular_serialized.as_slice());

        // Test deserialization works
        let deserialized = serializer
            .deserialize_message(pooled_buffer.as_slice())
            .unwrap();
        assert_eq!(message.from, deserialized.from);
    }
}
