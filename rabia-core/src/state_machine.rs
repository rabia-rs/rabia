use crate::{Command, Result};
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u64,
    pub data: Bytes,
    pub checksum: u32,
}

impl Snapshot {
    pub fn new(version: u64, data: impl Into<Bytes>) -> Self {
        let data = data.into();
        let checksum = crc32fast::hash(&data);
        Self {
            version,
            data,
            checksum,
        }
    }

    pub fn verify_checksum(&self) -> bool {
        crc32fast::hash(&self.data) == self.checksum
    }
}

#[async_trait]
pub trait StateMachine: Send + Sync {
    type State: Clone + Send + Sync;

    async fn apply_command(&mut self, command: &Command) -> Result<Bytes>;

    async fn apply_commands(&mut self, commands: &[Command]) -> Result<Vec<Bytes>> {
        let mut results = Vec::with_capacity(commands.len());
        for command in commands {
            results.push(self.apply_command(command).await?);
        }
        Ok(results)
    }

    async fn create_snapshot(&self) -> Result<Snapshot>;

    async fn restore_snapshot(&mut self, snapshot: &Snapshot) -> Result<()>;

    async fn get_state(&self) -> Self::State;

    fn is_deterministic(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub struct InMemoryStateMachine {
    pub state: std::collections::HashMap<String, Bytes>,
    pub version: u64,
}

impl InMemoryStateMachine {
    pub fn new() -> Self {
        Self {
            state: std::collections::HashMap::new(),
            version: 0,
        }
    }
}

impl Default for InMemoryStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateMachine for InMemoryStateMachine {
    type State = std::collections::HashMap<String, Bytes>;

    async fn apply_command(&mut self, command: &Command) -> Result<Bytes> {
        let command_str = String::from_utf8_lossy(&command.data);
        let parts: Vec<String> = command_str
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if parts.is_empty() {
            return Ok(Bytes::from("ERROR: Empty command"));
        }

        match parts[0].as_str() {
            "SET" if parts.len() == 3 => {
                let key = parts[1].clone();
                let value = Bytes::from(parts[2].clone());
                self.state.insert(key, value.clone());
                self.version += 1;
                Ok(Bytes::from("OK"))
            }
            "GET" if parts.len() == 2 => {
                let key = &parts[1];
                match self.state.get(key) {
                    Some(value) => Ok(value.clone()),
                    None => Ok(Bytes::from("NOT_FOUND")),
                }
            }
            "DEL" if parts.len() == 2 => {
                let key = &parts[1];
                match self.state.remove(key) {
                    Some(_) => {
                        self.version += 1;
                        Ok(Bytes::from("OK"))
                    }
                    None => Ok(Bytes::from("NOT_FOUND")),
                }
            }
            _ => Ok(Bytes::from("ERROR: Invalid command")),
        }
    }

    async fn create_snapshot(&self) -> Result<Snapshot> {
        let serialized = serde_json::to_vec(&self.state)?;
        Ok(Snapshot::new(self.version, serialized))
    }

    async fn restore_snapshot(&mut self, snapshot: &Snapshot) -> Result<()> {
        if !snapshot.verify_checksum() {
            return Err(crate::RabiaError::ChecksumMismatch {
                expected: snapshot.checksum,
                actual: crc32fast::hash(&snapshot.data),
            });
        }

        self.state = serde_json::from_slice(&snapshot.data)?;
        self.version = snapshot.version;
        Ok(())
    }

    async fn get_state(&self) -> Self::State {
        self.state.clone()
    }
}
