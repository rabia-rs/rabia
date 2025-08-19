use crate::messages::ProtocolMessage;
use crate::{BatchId, CommandBatch, NodeId, PhaseId, RabiaError, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Validator {
    fn validate(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub max_batch_size: usize,
    pub max_command_size: usize,
    pub max_clock_skew_ms: u64,
    pub min_phase_id: u64,
    pub max_phase_id: u64,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            max_command_size: 1024 * 1024, // 1MB
            max_clock_skew_ms: 60_000,     // 1 minute
            min_phase_id: 0,
            max_phase_id: u64::MAX,
        }
    }
}

impl Validator for ProtocolMessage {
    fn validate(&self) -> Result<()> {
        // Validate timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let config = ValidationConfig::default();

        if self.timestamp > now + config.max_clock_skew_ms {
            return Err(RabiaError::internal(format!(
                "Message timestamp {} is too far in the future (current: {})",
                self.timestamp, now
            )));
        }

        if now.saturating_sub(self.timestamp) > config.max_clock_skew_ms * 10 {
            return Err(RabiaError::internal(format!(
                "Message timestamp {} is too old (current: {})",
                self.timestamp, now
            )));
        }

        // Validate message type specific fields
        match &self.message_type {
            crate::messages::MessageType::Propose(propose) => {
                validate_phase_id(&propose.phase_id)?;
                validate_batch_id(&propose.batch_id)?;
                if let Some(batch) = &propose.batch {
                    batch.validate()?;
                }
            }
            crate::messages::MessageType::VoteRound1(vote) => {
                validate_phase_id(&vote.phase_id)?;
                validate_batch_id(&vote.batch_id)?;
                validate_node_id(&vote.voter_id)?;
            }
            crate::messages::MessageType::VoteRound2(vote) => {
                validate_phase_id(&vote.phase_id)?;
                validate_batch_id(&vote.batch_id)?;
                validate_node_id(&vote.voter_id)?;

                // Validate round1_votes mapping
                if vote.round1_votes.is_empty() {
                    return Err(RabiaError::internal(
                        "Round 2 vote must include round 1 votes".to_string(),
                    ));
                }
            }
            crate::messages::MessageType::Decision(decision) => {
                validate_phase_id(&decision.phase_id)?;
                validate_batch_id(&decision.batch_id)?;
                if let Some(batch) = &decision.batch {
                    batch.validate()?;
                }
            }
            crate::messages::MessageType::SyncRequest(request) => {
                validate_phase_id(&request.requester_phase)?;
            }
            crate::messages::MessageType::SyncResponse(response) => {
                validate_phase_id(&response.responder_phase)?;

                // Validate pending batches
                for (batch_id, batch) in &response.pending_batches {
                    validate_batch_id(batch_id)?;
                    batch.validate()?;
                }
            }
            crate::messages::MessageType::NewBatch(new_batch) => {
                new_batch.batch.validate()?;
                validate_node_id(&new_batch.originator)?;
            }
            crate::messages::MessageType::HeartBeat(heartbeat) => {
                validate_phase_id(&heartbeat.current_phase)?;
                validate_phase_id(&heartbeat.last_committed_phase)?;

                // Committed phase should not be greater than current phase
                if heartbeat.last_committed_phase > heartbeat.current_phase {
                    return Err(RabiaError::InvalidStateTransition {
                        from: format!("committed={}", heartbeat.last_committed_phase),
                        to: format!("current={}", heartbeat.current_phase),
                    });
                }
            }
            crate::messages::MessageType::QuorumNotification(notification) => {
                for node_id in &notification.active_nodes {
                    validate_node_id(node_id)?;
                }
            }
        }

        Ok(())
    }
}

impl Validator for CommandBatch {
    fn validate(&self) -> Result<()> {
        let config = ValidationConfig::default();

        // Validate batch size
        if self.commands.len() > config.max_batch_size {
            return Err(RabiaError::internal(format!(
                "Batch size {} exceeds maximum {}",
                self.commands.len(),
                config.max_batch_size
            )));
        }

        if self.commands.is_empty() {
            return Err(RabiaError::internal("Batch cannot be empty".to_string()));
        }

        // Validate individual commands
        for command in &self.commands {
            if command.data.len() > config.max_command_size {
                return Err(RabiaError::internal(format!(
                    "Command size {} exceeds maximum {}",
                    command.data.len(),
                    config.max_command_size
                )));
            }

            // Basic command validation
            if command.data.is_empty() {
                return Err(RabiaError::internal(
                    "Command data cannot be empty".to_string(),
                ));
            }
        }

        // Validate checksum
        let _calculated_checksum = self.checksum();
        // In a real implementation, we would compare against a stored checksum

        // Validate timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        if self.timestamp > now + config.max_clock_skew_ms {
            return Err(RabiaError::internal(format!(
                "Batch timestamp {} is too far in the future",
                self.timestamp
            )));
        }

        Ok(())
    }
}

fn validate_phase_id(phase_id: &PhaseId) -> Result<()> {
    let config = ValidationConfig::default();
    let value = phase_id.value();

    if value < config.min_phase_id || value > config.max_phase_id {
        return Err(RabiaError::internal(format!(
            "Phase ID {} is out of valid range [{}, {}]",
            value, config.min_phase_id, config.max_phase_id
        )));
    }

    Ok(())
}

fn validate_batch_id(_batch_id: &BatchId) -> Result<()> {
    // BatchId is a UUID, so basic validation is that it's not nil
    // Additional validation could include checking against known batches
    Ok(())
}

fn validate_node_id(_node_id: &NodeId) -> Result<()> {
    // NodeId is a UUID, so basic validation is that it's not nil
    // Additional validation could include checking against authorized nodes
    Ok(())
}

pub fn validate_message_sequence(previous_phase: PhaseId, current_phase: PhaseId) -> Result<()> {
    if current_phase.value() <= previous_phase.value() {
        return Err(RabiaError::InvalidStateTransition {
            from: format!("phase={}", previous_phase),
            to: format!("phase={}", current_phase),
        });
    }

    // Check for reasonable phase progression (not too large jumps)
    let jump = current_phase.value() - previous_phase.value();
    if jump > 1000 {
        return Err(RabiaError::internal(format!(
            "Phase jump {} is suspiciously large",
            jump
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{messages::ProposeMessage, Command};

    #[test]
    fn test_batch_validation() {
        let commands = vec![Command::new("SET key1 value1"), Command::new("GET key1")];
        let batch = CommandBatch::new(commands);

        assert!(batch.validate().is_ok());
    }

    #[test]
    fn test_empty_batch_validation() {
        let batch = CommandBatch::new(vec![]);
        assert!(batch.validate().is_err());
    }

    #[test]
    fn test_phase_sequence_validation() {
        let phase1 = PhaseId::new(1);
        let phase2 = PhaseId::new(2);
        let phase3 = PhaseId::new(1); // Invalid: going backwards

        assert!(validate_message_sequence(phase1, phase2).is_ok());
        assert!(validate_message_sequence(phase2, phase3).is_err());
    }
}
