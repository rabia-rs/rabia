//! # Core Types
//!
//! Fundamental types used throughout the Rabia consensus protocol.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for a node in the consensus cluster.
///
/// Each node participating in the Rabia consensus protocol has a unique identifier
/// that is generated when the node starts. This identifier is used for message
/// routing, leader election, and membership management.
///
/// # Examples
///
/// ```rust
/// use rabia_core::NodeId;
///
/// let node_id = NodeId::new();
/// println!("Node ID: {}", node_id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

impl NodeId {
    /// Creates a new random node identifier.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::NodeId;
    ///
    /// let node_id = NodeId::new();
    /// assert_ne!(node_id, NodeId::new()); // Should be unique
    /// ```
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<u32> for NodeId {
    /// Creates a NodeId from a u32 for testing and examples.
    ///
    /// This creates a deterministic UUID based on the input number,
    /// which is useful for testing and examples where predictable
    /// node IDs are needed.
    fn from(value: u32) -> Self {
        // Create a deterministic UUID from the u32 value
        // Use the value in the first 4 bytes and repeat the pattern
        let bytes = [
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ];
        Self(Uuid::from_bytes(bytes))
    }
}

impl From<u64> for NodeId {
    /// Creates a NodeId from a u64 for testing and examples.
    ///
    /// This creates a deterministic UUID based on the input number,
    /// which is useful for testing and examples where predictable
    /// node IDs are needed.
    fn from(value: u64) -> Self {
        // Create a deterministic UUID from the u64 value
        // Use the value in the first 8 bytes and repeat the pattern
        let bytes = [
            (value >> 56) as u8,
            (value >> 48) as u8,
            (value >> 40) as u8,
            (value >> 32) as u8,
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
            (value >> 56) as u8,
            (value >> 48) as u8,
            (value >> 40) as u8,
            (value >> 32) as u8,
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ];
        Self(Uuid::from_bytes(bytes))
    }
}

impl From<i32> for NodeId {
    /// Creates a NodeId from an i32 for testing and examples.
    ///
    /// This creates a deterministic UUID based on the input number,
    /// which is useful for testing and examples where predictable
    /// node IDs are needed.
    fn from(value: i32) -> Self {
        Self::from(value as u32)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// State of a node's participation in consensus.
///
/// Tracks whether a node is actively participating in the consensus protocol
/// or is currently idle/inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConsensusState {
    /// Node is actively participating in consensus
    Active,
    /// Node is not currently participating in consensus
    Idle,
}

impl fmt::Display for ConsensusState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConsensusState::Active => write!(f, "Active"),
            ConsensusState::Idle => write!(f, "Idle"),
        }
    }
}

/// Identifier for consensus phases in the Rabia protocol.
///
/// The Rabia consensus protocol operates in phases, where each phase represents
/// a step in the consensus process. Phase IDs are monotonically increasing
/// and used to order consensus rounds.
///
/// # Examples
///
/// ```rust
/// use rabia_core::PhaseId;
///
/// let phase1 = PhaseId::new(1);
/// let phase2 = phase1.next();
/// assert!(phase2 > phase1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PhaseId(pub u64);

impl PhaseId {
    /// Creates a new phase identifier with the given value.
    ///
    /// # Arguments
    ///
    /// * `id` - The numeric phase identifier
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::PhaseId;
    ///
    /// let phase = PhaseId::new(42);
    /// assert_eq!(phase.value(), 42);
    /// ```
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the next phase in sequence.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::PhaseId;
    ///
    /// let phase1 = PhaseId::new(5);
    /// let phase2 = phase1.next();
    /// assert_eq!(phase2.value(), 6);
    /// ```
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    /// Returns the numeric value of this phase ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::PhaseId;
    ///
    /// let phase = PhaseId::new(100);
    /// assert_eq!(phase.value(), 100);
    /// ```
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for PhaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for command batches in consensus.
///
/// Each batch of commands submitted for consensus has a unique identifier
/// that is used to track the batch through the consensus process and
/// ensure idempotency.
///
/// # Examples
///
/// ```rust
/// use rabia_core::BatchId;
///
/// let batch_id = BatchId::new();
/// println!("Batch ID: {}", batch_id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchId(pub Uuid);

impl BatchId {
    /// Creates a new random batch identifier.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::BatchId;
    ///
    /// let batch_id = BatchId::new();
    /// assert_ne!(batch_id, BatchId::new()); // Should be unique
    /// ```
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BatchId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// State values used in the Rabia consensus protocol.
///
/// These values represent the possible states that a node can vote for
/// during the consensus process. The Rabia protocol uses randomization
/// to break ties and ensure progress.
///
/// # Values
///
/// * `V0` - Vote for state 0 (reject)
/// * `V1` - Vote for state 1 (accept)
/// * `VQuestion` - Undecided vote (used in randomization)
///
/// # Examples
///
/// ```rust
/// use rabia_core::StateValue;
///
/// let vote = StateValue::V1;
/// println!("Vote: {}", vote);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateValue {
    /// Vote to reject the proposed value
    V0,
    /// Vote to accept the proposed value
    V1,
    /// Undecided vote, used in randomization phase
    VQuestion,
}

impl fmt::Display for StateValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateValue::V0 => write!(f, "V0"),
            StateValue::V1 => write!(f, "V1"),
            StateValue::VQuestion => write!(f, "V?"),
        }
    }
}

/// A command to be executed by the state machine.
///
/// Commands represent individual operations that can be applied to the
/// distributed state machine. Each command has a unique identifier and
/// contains arbitrary data that will be interpreted by the state machine.
///
/// # Examples
///
/// ```rust
/// use rabia_core::Command;
///
/// let cmd = Command::new("SET key value");
/// println!("Command ID: {}", cmd.id);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Unique identifier for this command
    pub id: Uuid,
    /// Command data to be executed by the state machine
    pub data: bytes::Bytes,
}

impl Command {
    /// Creates a new command with the given data.
    ///
    /// The command will be assigned a unique random identifier.
    ///
    /// # Arguments
    ///
    /// * `data` - The command data (can be string, bytes, etc.)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::Command;
    ///
    /// let cmd1 = Command::new("GET key1");
    /// let cmd2 = Command::new(b"SET key2 value2".as_slice());
    /// ```
    pub fn new(data: impl Into<bytes::Bytes>) -> Self {
        Self {
            id: Uuid::new_v4(),
            data: data.into(),
        }
    }
}

/// A batch of commands to be processed together.
///
/// Command batching improves throughput by amortizing the consensus overhead
/// across multiple commands. Each batch has a unique identifier and timestamp
/// for tracking and ordering.
///
/// # Examples
///
/// ```rust
/// use rabia_core::{Command, CommandBatch};
///
/// let commands = vec![
///     Command::new("SET key1 value1"),
///     Command::new("SET key2 value2"),
/// ];
/// let batch = CommandBatch::new(commands);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandBatch {
    /// Unique identifier for this batch
    pub id: BatchId,
    /// Commands included in this batch
    pub commands: Vec<Command>,
    /// Timestamp when the batch was created (milliseconds since Unix epoch)
    pub timestamp: u64,
}

impl CommandBatch {
    /// Creates a new command batch with the given commands.
    ///
    /// The batch will be assigned a unique identifier and the current timestamp.
    ///
    /// # Arguments
    ///
    /// * `commands` - Vector of commands to include in the batch
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::{Command, CommandBatch};
    ///
    /// let commands = vec![
    ///     Command::new("SET key1 value1"),
    ///     Command::new("GET key1"),
    /// ];
    /// let batch = CommandBatch::new(commands);
    /// assert_eq!(batch.commands.len(), 2);
    /// ```
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            id: BatchId::new(),
            commands,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }

    /// Calculates a checksum for the batch to verify integrity.
    ///
    /// This checksum can be used to detect corruption or ensure that
    /// the same batch is being processed by all nodes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rabia_core::{Command, CommandBatch};
    ///
    /// let batch = CommandBatch::new(vec![Command::new("test")]);
    /// let checksum = batch.checksum();
    /// assert!(checksum > 0);
    /// ```
    pub fn checksum(&self) -> u32 {
        let serialized = serde_json::to_vec(self).unwrap_or_default();
        crc32fast::hash(&serialized)
    }
}
