use bytes::Bytes;
use dashmap::DashMap;
use parking_lot::RwLock;
use rabia_core::{
    messages::{PendingBatch, PhaseData, SyncResponseMessage},
    BatchId, CommandBatch, NodeId, PhaseId, RabiaError, Result,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub struct EngineState {
    pub current_phase: Arc<AtomicU64>,
    pub last_committed_phase: Arc<AtomicU64>,
    pub is_active: Arc<AtomicBool>,
    pub has_quorum: Arc<AtomicBool>,

    pub pending_batches: Arc<DashMap<BatchId, PendingBatch>>,
    pub phases: Arc<DashMap<PhaseId, PhaseData>>,
    pub sync_responses: Arc<DashMap<NodeId, SyncResponseMessage>>,

    pub active_nodes: Arc<RwLock<std::collections::HashSet<NodeId>>>,
    pub quorum_size: usize,

    pub state_version: Arc<AtomicU64>,
    pub last_cleanup: Arc<AtomicU64>,
}

impl EngineState {
    pub fn new(quorum_size: usize) -> Self {
        Self {
            current_phase: Arc::new(AtomicU64::new(0)),
            last_committed_phase: Arc::new(AtomicU64::new(0)),
            is_active: Arc::new(AtomicBool::new(true)),
            has_quorum: Arc::new(AtomicBool::new(true)),

            pending_batches: Arc::new(DashMap::new()),
            phases: Arc::new(DashMap::new()),
            sync_responses: Arc::new(DashMap::new()),

            active_nodes: Arc::new(RwLock::new(std::collections::HashSet::new())),
            quorum_size,

            state_version: Arc::new(AtomicU64::new(1)),
            last_cleanup: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn current_phase(&self) -> PhaseId {
        PhaseId::new(self.current_phase.load(Ordering::Acquire))
    }

    pub fn last_committed_phase(&self) -> PhaseId {
        PhaseId::new(self.last_committed_phase.load(Ordering::Acquire))
    }

    pub fn advance_phase(&self) -> PhaseId {
        let new_phase = self.current_phase.fetch_add(1, Ordering::AcqRel) + 1;
        self.increment_version();
        PhaseId::new(new_phase)
    }

    pub fn commit_phase(&self, phase_id: PhaseId) -> Result<bool> {
        let phase_value = phase_id.value();
        let current_phase_value = self.current_phase.load(Ordering::Acquire);

        // Validate phase ordering - can only commit phases <= current phase
        if phase_value > current_phase_value {
            return Err(RabiaError::InvalidStateTransition {
                from: format!("current_phase={}", current_phase_value),
                to: format!("commit_phase={}", phase_value),
            });
        }

        let mut current = self.last_committed_phase.load(Ordering::Acquire);

        // Only allow monotonic increases in committed phase
        while current < phase_value {
            match self.last_committed_phase.compare_exchange_weak(
                current,
                phase_value,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.increment_version();
                    return Ok(true);
                }
                Err(actual) => {
                    current = actual;
                    // If someone else committed a higher phase, we're done
                    if current >= phase_value {
                        return Ok(false);
                    }
                }
            }
        }

        // Phase was already committed or higher phase was committed
        Ok(false)
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Acquire)
    }

    pub fn set_active(&self, active: bool) {
        if self.is_active.swap(active, Ordering::AcqRel) != active {
            self.increment_version();
        }
    }

    pub fn has_quorum(&self) -> bool {
        self.has_quorum.load(Ordering::Acquire)
    }

    pub fn set_quorum(&self, has_quorum: bool) {
        if self.has_quorum.swap(has_quorum, Ordering::AcqRel) != has_quorum {
            self.increment_version();
        }
    }

    pub fn get_active_nodes(&self) -> std::collections::HashSet<NodeId> {
        self.active_nodes.read().clone()
    }

    pub fn update_active_nodes(&self, nodes: std::collections::HashSet<NodeId>) {
        let has_quorum = nodes.len() >= self.quorum_size;

        {
            let mut active_nodes = self.active_nodes.write();
            if *active_nodes != nodes {
                *active_nodes = nodes;
                self.increment_version();
            }
        }

        self.set_quorum(has_quorum);
        self.set_active(has_quorum);
    }

    pub fn add_pending_batch(&self, batch: CommandBatch, originator: NodeId) -> BatchId {
        let pending = PendingBatch::new(batch, originator);
        let batch_id = pending.batch.id;
        self.pending_batches.insert(batch_id, pending);
        self.increment_version();
        batch_id
    }

    pub fn remove_pending_batch(&self, batch_id: &BatchId) -> Option<PendingBatch> {
        let result = self.pending_batches.remove(batch_id).map(|(_, v)| v);
        if result.is_some() {
            self.increment_version();
        }
        result
    }

    pub fn get_pending_batch(&self, batch_id: &BatchId) -> Option<PendingBatch> {
        self.pending_batches
            .get(batch_id)
            .map(|entry| entry.value().clone())
    }

    pub fn get_or_create_phase(&self, phase_id: PhaseId) -> PhaseData {
        self.phases
            .entry(phase_id)
            .or_insert_with(|| {
                self.increment_version();
                PhaseData::new(phase_id)
            })
            .clone()
    }

    pub fn update_phase<F>(&self, phase_id: PhaseId, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut PhaseData),
    {
        if let Some(mut entry) = self.phases.get_mut(&phase_id) {
            update_fn(&mut entry);
            self.increment_version();
        }
        Ok(())
    }

    pub fn get_phase(&self, phase_id: &PhaseId) -> Option<PhaseData> {
        self.phases.get(phase_id).map(|entry| entry.value().clone())
    }

    pub fn cleanup_old_phases(&self, max_phase_history: usize) -> usize {
        let current_phase = self.current_phase();
        let cutoff_phase = if current_phase.value() > max_phase_history as u64 {
            PhaseId::new(current_phase.value() - max_phase_history as u64)
        } else {
            PhaseId::new(0)
        };

        let mut removed_count = 0;
        self.phases.retain(|&phase_id, _| {
            let should_keep = phase_id >= cutoff_phase;
            if !should_keep {
                removed_count += 1;
            }
            should_keep
        });

        if removed_count > 0 {
            self.increment_version();
            self.last_cleanup.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                Ordering::Release,
            );
        }

        removed_count
    }

    pub fn cleanup_old_pending_batches(&self, max_age_secs: u64) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let cutoff = now.saturating_sub(max_age_secs * 1000);

        let mut removed_count = 0;
        self.pending_batches.retain(|_, pending| {
            let should_keep = pending.received_timestamp >= cutoff;
            if !should_keep {
                removed_count += 1;
            }
            should_keep
        });

        if removed_count > 0 {
            self.increment_version();
        }

        removed_count
    }

    pub fn get_state_version(&self) -> u64 {
        self.state_version.load(Ordering::Acquire)
    }

    fn increment_version(&self) {
        self.state_version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn add_sync_response(&self, node_id: NodeId, response: SyncResponseMessage) {
        self.sync_responses.insert(node_id, response);
    }

    pub fn get_sync_responses(&self) -> HashMap<NodeId, SyncResponseMessage> {
        self.sync_responses
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }

    pub fn clear_sync_responses(&self) {
        self.sync_responses.clear();
    }

    pub fn get_statistics(&self) -> EngineStatistics {
        EngineStatistics {
            current_phase: self.current_phase(),
            last_committed_phase: self.last_committed_phase(),
            pending_batches_count: self.pending_batches.len(),
            phases_count: self.phases.len(),
            active_nodes_count: self.active_nodes.read().len(),
            has_quorum: self.has_quorum(),
            is_active: self.is_active(),
            state_version: self.get_state_version(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineStatistics {
    pub current_phase: PhaseId,
    pub last_committed_phase: PhaseId,
    pub pending_batches_count: usize,
    pub phases_count: usize,
    pub active_nodes_count: usize,
    pub has_quorum: bool,
    pub is_active: bool,
    pub state_version: u64,
}

#[derive(Debug)]
pub struct CommandRequest {
    pub batch: CommandBatch,
    pub response_tx: oneshot::Sender<Result<Vec<Bytes>>>,
}

#[derive(Debug)]
pub enum EngineCommand {
    ProcessBatch(CommandRequest),
    Shutdown,
    ForcePhaseAdvance,
    TriggerSync,
    GetStatistics(oneshot::Sender<EngineStatistics>),
}

pub type EngineCommandSender = mpsc::UnboundedSender<EngineCommand>;
pub type EngineCommandReceiver = mpsc::UnboundedReceiver<EngineCommand>;
