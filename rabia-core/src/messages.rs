use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::{NodeId, PhaseId, BatchId, CommandBatch, StateValue};
use crate::state_machine::Snapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub id: uuid::Uuid,
    pub from: NodeId,
    pub to: Option<NodeId>, // None for broadcast
    pub timestamp: u64,
    pub message_type: MessageType,
}

impl ProtocolMessage {
    pub fn new(from: NodeId, to: Option<NodeId>, message_type: MessageType) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            from,
            to,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            message_type,
        }
    }

    pub fn propose(from: NodeId, proposal: ProposeMessage) -> Self {
        Self::new(from, None, MessageType::Propose(proposal))
    }

    pub fn vote_round1(from: NodeId, to: NodeId, vote: VoteRound1Message) -> Self {
        Self::new(from, Some(to), MessageType::VoteRound1(vote))
    }

    pub fn vote_round2(from: NodeId, to: NodeId, vote: VoteRound2Message) -> Self {
        Self::new(from, Some(to), MessageType::VoteRound2(vote))
    }

    pub fn decision(from: NodeId, decision: DecisionMessage) -> Self {
        Self::new(from, None, MessageType::Decision(decision))
    }

    pub fn sync_request(from: NodeId, to: NodeId, request: SyncRequestMessage) -> Self {
        Self::new(from, Some(to), MessageType::SyncRequest(request))
    }

    pub fn sync_response(from: NodeId, to: NodeId, response: SyncResponseMessage) -> Self {
        Self::new(from, Some(to), MessageType::SyncResponse(response))
    }

    pub fn new_batch(from: NodeId, batch: NewBatchMessage) -> Self {
        Self::new(from, None, MessageType::NewBatch(batch))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Propose(ProposeMessage),
    VoteRound1(VoteRound1Message),
    VoteRound2(VoteRound2Message),
    Decision(DecisionMessage),
    SyncRequest(SyncRequestMessage),
    SyncResponse(SyncResponseMessage),
    NewBatch(NewBatchMessage),
    HeartBeat(HeartBeatMessage),
    QuorumNotification(QuorumNotificationMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeMessage {
    pub phase_id: PhaseId,
    pub batch_id: BatchId,
    pub value: StateValue,
    pub batch: Option<CommandBatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRound1Message {
    pub phase_id: PhaseId,
    pub batch_id: BatchId,
    pub vote: StateValue,
    pub voter_id: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRound2Message {
    pub phase_id: PhaseId,
    pub batch_id: BatchId,
    pub vote: StateValue,
    pub voter_id: NodeId,
    pub round1_votes: HashMap<NodeId, StateValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionMessage {
    pub phase_id: PhaseId,
    pub batch_id: BatchId,
    pub decision: StateValue,
    pub batch: Option<CommandBatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequestMessage {
    pub requester_phase: PhaseId,
    pub requester_state_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponseMessage {
    pub responder_phase: PhaseId,
    pub responder_state_version: u64,
    pub state_snapshot: Option<Snapshot>,
    pub pending_batches: Vec<(BatchId, CommandBatch)>,
    pub committed_phases: Vec<(PhaseId, BatchId, StateValue)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewBatchMessage {
    pub batch: CommandBatch,
    pub originator: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartBeatMessage {
    pub current_phase: PhaseId,
    pub last_committed_phase: PhaseId,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumNotificationMessage {
    pub has_quorum: bool,
    pub active_nodes: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseData {
    pub phase_id: PhaseId,
    pub batch_id: Option<BatchId>,
    pub proposed_value: Option<StateValue>,
    pub round1_votes: HashMap<NodeId, StateValue>,
    pub round2_votes: HashMap<NodeId, StateValue>,
    pub decision: Option<StateValue>,
    pub batch: Option<CommandBatch>,
    pub timestamp: u64,
    pub is_committed: bool,
}

impl PhaseData {
    pub fn new(phase_id: PhaseId) -> Self {
        Self {
            phase_id,
            batch_id: None,
            proposed_value: None,
            round1_votes: HashMap::new(),
            round2_votes: HashMap::new(),
            decision: None,
            batch: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            is_committed: false,
        }
    }

    pub fn add_round1_vote(&mut self, voter: NodeId, vote: StateValue) {
        self.round1_votes.insert(voter, vote);
    }

    pub fn add_round2_vote(&mut self, voter: NodeId, vote: StateValue) {
        self.round2_votes.insert(voter, vote);
    }

    pub fn has_round1_majority(&self, quorum_size: usize) -> Option<StateValue> {
        self.count_votes(&self.round1_votes, quorum_size)
    }

    pub fn has_round2_majority(&self, quorum_size: usize) -> Option<StateValue> {
        self.count_votes(&self.round2_votes, quorum_size)
    }

    fn count_votes(&self, votes: &HashMap<NodeId, StateValue>, quorum_size: usize) -> Option<StateValue> {
        let mut v0_count = 0;
        let mut v1_count = 0;
        let mut vq_count = 0;

        for vote in votes.values() {
            match vote {
                StateValue::V0 => v0_count += 1,
                StateValue::V1 => v1_count += 1,
                StateValue::VQuestion => vq_count += 1,
            }
        }

        if v0_count >= quorum_size {
            Some(StateValue::V0)
        } else if v1_count >= quorum_size {
            Some(StateValue::V1)
        } else if vq_count >= quorum_size {
            Some(StateValue::VQuestion)
        } else {
            None
        }
    }

    pub fn total_votes(&self) -> usize {
        self.round1_votes.len().max(self.round2_votes.len())
    }

    pub fn set_decision(&mut self, decision: StateValue) {
        self.decision = Some(decision.clone());
        if decision != StateValue::VQuestion {
            self.is_committed = true;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBatch {
    pub batch: CommandBatch,
    pub originator: NodeId,
    pub received_timestamp: u64,
    pub retry_count: usize,
}

impl PendingBatch {
    pub fn new(batch: CommandBatch, originator: NodeId) -> Self {
        Self {
            batch,
            originator,
            received_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            retry_count: 0,
        }
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    pub fn age_millis(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now.saturating_sub(self.received_timestamp)
    }
}