use rand::Rng;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};

use rabia_core::{
    messages::{
        DecisionMessage, HeartBeatMessage, MessageType, NewBatchMessage, ProposeMessage,
        ProtocolMessage, SyncRequestMessage, SyncResponseMessage, VoteRound1Message,
        VoteRound2Message,
    },
    network::{ClusterConfig, NetworkEventHandler, NetworkTransport},
    persistence::PersistenceLayer,
    state_machine::StateMachine,
    BatchId, CommandBatch, NodeId, PhaseId, RabiaError, Result, StateValue, Validator,
};

use crate::{CommandRequest, EngineCommand, EngineCommandReceiver, EngineState, RabiaConfig};

pub struct RabiaEngine<SM, NT, PL>
where
    SM: StateMachine + 'static,
    NT: NetworkTransport + 'static,
    PL: PersistenceLayer + 'static,
{
    node_id: NodeId,
    config: RabiaConfig,
    #[allow(dead_code)]
    cluster_config: ClusterConfig,
    state_machine: Arc<tokio::sync::Mutex<SM>>,
    network: Arc<tokio::sync::Mutex<NT>>,
    persistence: Arc<PL>,
    engine_state: Arc<EngineState>,
    command_rx: EngineCommandReceiver,
    rng: rand::rngs::StdRng,
}

impl<SM, NT, PL> RabiaEngine<SM, NT, PL>
where
    SM: StateMachine + 'static,
    NT: NetworkTransport + 'static,
    PL: PersistenceLayer + 'static,
{
    pub fn new(
        node_id: NodeId,
        config: RabiaConfig,
        cluster_config: ClusterConfig,
        state_machine: SM,
        network: NT,
        persistence: PL,
        command_rx: EngineCommandReceiver,
    ) -> Self {
        let rng = match config.randomization_seed {
            Some(seed) => rand::SeedableRng::seed_from_u64(seed),
            None => rand::SeedableRng::from_entropy(),
        };

        Self {
            node_id,
            config: config.clone(),
            cluster_config: cluster_config.clone(),
            state_machine: Arc::new(tokio::sync::Mutex::new(state_machine)),
            network: Arc::new(tokio::sync::Mutex::new(network)),
            persistence: Arc::new(persistence),
            engine_state: Arc::new(EngineState::new(cluster_config.quorum_size)),
            command_rx,
            rng,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Starting Rabia consensus engine for node {}", self.node_id);

        let mut cleanup_interval = interval(self.config.cleanup_interval);
        let mut heartbeat_interval = interval(self.config.heartbeat_interval);
        let mut message_buffer = Vec::new();

        self.initialize().await?;

        loop {
            // Try to receive messages first
            if let Err(e) = self.receive_messages(&mut message_buffer).await {
                if !e.to_string().contains("No messages available") {
                    error!("Error receiving messages: {}", e);
                }
            } else {
                for (from, message) in message_buffer.drain(..) {
                    if let Err(e) = self.handle_message(from, message).await {
                        error!("Error handling message from {}: {}", from, e);
                    }
                }
            }

            tokio::select! {
                // Handle incoming commands
                command_opt = self.command_rx.recv() => {
                    if let Some(command) = command_opt {
                        if let Err(e) = self.handle_command(command).await {
                            error!("Error handling command: {}", e);
                        }
                    } else {
                        // Channel closed, exit loop
                        break Ok(());
                    }
                }

                // Cleanup old state
                _ = cleanup_interval.tick() => {
                    self.cleanup_old_state().await;
                }

                // Send heartbeats
                _ = heartbeat_interval.tick() => {
                    if let Err(e) = self.send_heartbeat().await {
                        warn!("Failed to send heartbeat: {}", e);
                    }
                }

                // Prevent busy waiting
                _ = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        // Try to restore state from persistence
        if let Some(persisted_state) = self.persistence.load_state().await? {
            info!("Restoring state from persistence");

            // Restore engine state
            self.engine_state.current_phase.store(
                persisted_state.current_phase.value(),
                std::sync::atomic::Ordering::Release,
            );
            self.engine_state.last_committed_phase.store(
                persisted_state.last_committed_phase.value(),
                std::sync::atomic::Ordering::Release,
            );

            // Restore state machine if snapshot exists
            if let Some(snapshot) = persisted_state.snapshot {
                let mut sm = self.state_machine.lock().await;
                sm.restore_snapshot(&snapshot).await?;
            }
        }

        // Initialize network connections
        let connected_nodes = self.network.lock().await.get_connected_nodes().await?;
        self.engine_state.update_active_nodes(connected_nodes);

        info!("Engine initialized successfully");
        Ok(())
    }

    async fn handle_command(&mut self, command: EngineCommand) -> Result<()> {
        match command {
            EngineCommand::ProcessBatch(request) => self.process_batch_request(request).await,
            EngineCommand::Shutdown => {
                info!("Shutting down consensus engine");
                Err(RabiaError::internal("Shutdown requested"))
            }
            EngineCommand::ForcePhaseAdvance => self.advance_to_next_phase().await,
            EngineCommand::TriggerSync => self.initiate_sync().await,
            EngineCommand::GetStatistics(tx) => {
                let stats = self.engine_state.get_statistics();
                let _ = tx.send(stats);
                Ok(())
            }
        }
    }

    async fn process_batch_request(&mut self, request: CommandRequest) -> Result<()> {
        if !self.engine_state.has_quorum() {
            let _ = request
                .response_tx
                .send(Err(RabiaError::QuorumNotAvailable {
                    current: self.engine_state.get_active_nodes().len(),
                    required: self.engine_state.quorum_size,
                }));
            return Ok(());
        }

        // Add batch to pending
        let batch_id = self
            .engine_state
            .add_pending_batch(request.batch.clone(), self.node_id);

        // Start consensus for this batch
        self.propose_batch(batch_id, request.batch).await?;

        // Note: The response will be sent when consensus completes
        // For now, we'll store the response channel in the pending batch
        Ok(())
    }

    async fn propose_batch(&mut self, batch_id: BatchId, batch: CommandBatch) -> Result<()> {
        let phase_id = self.engine_state.advance_phase();

        debug!("Proposing batch {} in phase {}", batch_id, phase_id);

        // Randomly choose initial value (key aspect of Rabia protocol)
        let initial_value = if self.rng.gen_bool(0.5) {
            StateValue::V0
        } else {
            StateValue::V1
        };

        // Update phase with proposal
        self.engine_state.update_phase(phase_id, |phase| {
            phase.batch_id = Some(batch_id);
            phase.proposed_value = Some(initial_value);
            phase.batch = Some(batch.clone());
        })?;

        // Broadcast proposal
        let proposal = ProposeMessage {
            phase_id,
            batch_id,
            value: initial_value,
            batch: Some(batch),
        };

        let message = ProtocolMessage::propose(self.node_id, proposal);
        self.network
            .lock()
            .await
            .broadcast(message, Some(self.node_id))
            .await?;

        Ok(())
    }

    async fn handle_message(&mut self, from: NodeId, message: ProtocolMessage) -> Result<()> {
        // Validate incoming message
        if let Err(e) = message.validate() {
            warn!("Received invalid message from {}: {}", from, e);
            return Err(e);
        }

        // Validate message source
        if message.from != from {
            warn!(
                "Message claims to be from {} but received from {}",
                message.from, from
            );
            return Err(RabiaError::network("Message source mismatch"));
        }

        match message.message_type {
            MessageType::Propose(propose) => self.handle_propose(from, propose).await,
            MessageType::VoteRound1(vote) => self.handle_vote_round1(from, vote).await,
            MessageType::VoteRound2(vote) => self.handle_vote_round2(from, vote).await,
            MessageType::Decision(decision) => self.handle_decision(from, decision).await,
            MessageType::SyncRequest(request) => self.handle_sync_request(from, request).await,
            MessageType::SyncResponse(response) => self.handle_sync_response(from, response).await,
            MessageType::NewBatch(new_batch) => self.handle_new_batch(from, new_batch).await,
            MessageType::HeartBeat(heartbeat) => self.handle_heartbeat(from, heartbeat).await,
            MessageType::QuorumNotification(_) => {
                // Handle quorum notifications
                Ok(())
            }
        }
    }

    async fn handle_propose(&mut self, from: NodeId, propose: ProposeMessage) -> Result<()> {
        if !self.engine_state.has_quorum() {
            return Ok(()); // Ignore proposals when no quorum
        }

        debug!(
            "Received proposal from {} for phase {}",
            from, propose.phase_id
        );

        // Store the batch if we don't have it
        if let Some(batch) = &propose.batch {
            self.engine_state.add_pending_batch(batch.clone(), from);
        }

        // Determine our vote for round 1
        let vote = self.determine_round1_vote(&propose).await;

        // Update phase data
        self.engine_state.update_phase(propose.phase_id, |phase| {
            phase.batch_id = Some(propose.batch_id);
            if phase.proposed_value.is_none() {
                phase.proposed_value = Some(propose.value);
            }
            if phase.batch.is_none() {
                phase.batch = propose.batch.clone();
            }
        })?;

        // Send round 1 vote
        let vote_msg = VoteRound1Message {
            phase_id: propose.phase_id,
            batch_id: propose.batch_id,
            vote,
            voter_id: self.node_id,
        };

        let message = ProtocolMessage::vote_round1(self.node_id, from, vote_msg);
        self.network.lock().await.send_to(from, message).await?;

        Ok(())
    }

    async fn determine_round1_vote(&mut self, propose: &ProposeMessage) -> StateValue {
        // Rabia's voting strategy: nodes vote based on their local state and randomization
        // This implements the Rabia protocol's voting rules for round 1

        // Check if we have any conflicting proposals for this phase
        let phase = self.engine_state.get_phase(&propose.phase_id);

        match phase {
            Some(existing_phase) => {
                // If we already have a proposal for this phase
                if let Some(existing_value) = &existing_phase.proposed_value {
                    if *existing_value == propose.value {
                        // Same proposal - vote for it
                        propose.value
                    } else {
                        // Conflicting proposal - vote ? (uncertain)
                        StateValue::VQuestion
                    }
                } else {
                    // First time seeing this phase - randomized vote
                    self.randomized_vote(&propose.value)
                }
            }
            None => {
                // New phase - randomized vote based on Rabia protocol
                self.randomized_vote(&propose.value)
            }
        }
    }

    fn randomized_vote(&mut self, proposed_value: &StateValue) -> StateValue {
        // Rabia's randomized voting: bias towards V1 for liveness
        // while maintaining safety through randomization
        match proposed_value {
            StateValue::V0 => {
                // For V0 proposals, vote V0 with probability 0.5, else VQuestion
                if self.rng.gen_bool(0.5) {
                    StateValue::V0
                } else {
                    StateValue::VQuestion
                }
            }
            StateValue::V1 => {
                // For V1 proposals, vote V1 with higher probability for liveness
                if self.rng.gen_bool(0.6) {
                    StateValue::V1
                } else {
                    StateValue::VQuestion
                }
            }
            StateValue::VQuestion => {
                // For uncertain proposals, default to VQuestion
                StateValue::VQuestion
            }
        }
    }

    async fn handle_vote_round1(&mut self, from: NodeId, vote: VoteRound1Message) -> Result<()> {
        debug!(
            "Received round 1 vote from {} for phase {}",
            from, vote.phase_id
        );

        // Update phase with vote
        self.engine_state.update_phase(vote.phase_id, |phase| {
            phase.add_round1_vote(from, vote.vote);
        })?;

        // Check if we have enough votes to proceed to round 2
        if let Some(phase) = self.engine_state.get_phase(&vote.phase_id) {
            if let Some(majority_vote) = phase.has_round1_majority(self.engine_state.quorum_size) {
                self.proceed_to_round2(vote.phase_id, majority_vote, phase.round1_votes)
                    .await?;
            }
        }

        Ok(())
    }

    async fn proceed_to_round2(
        &mut self,
        phase_id: PhaseId,
        round1_result: StateValue,
        round1_votes: std::collections::HashMap<NodeId, StateValue>,
    ) -> Result<()> {
        debug!(
            "Proceeding to round 2 for phase {} with result {:?}",
            phase_id, round1_result
        );

        // Rabia protocol round 2 voting rules
        let round2_vote = match round1_result {
            StateValue::V0 => {
                // Round 1 decided V0 - must vote V0 for safety
                StateValue::V0
            }
            StateValue::V1 => {
                // Round 1 decided V1 - must vote V1 for safety
                StateValue::V1
            }
            StateValue::VQuestion => {
                // Round 1 was inconclusive - Rabia's randomized choice
                // Bias towards V1 for liveness while maintaining safety
                self.determine_round2_vote_for_question(&round1_votes)
            }
        };

        // Update our phase with round 2 vote
        self.engine_state.update_phase(phase_id, |phase| {
            phase.add_round2_vote(self.node_id, round2_vote);
        })?;

        // Broadcast round 2 vote
        let vote_msg = VoteRound2Message {
            phase_id,
            batch_id: self
                .engine_state
                .get_phase(&phase_id)
                .and_then(|p| p.batch_id)
                .unwrap_or_default(),
            vote: round2_vote,
            voter_id: self.node_id,
            round1_votes,
        };

        let message = ProtocolMessage::vote_round2(self.node_id, self.node_id, vote_msg);
        self.network
            .lock()
            .await
            .broadcast(message, Some(self.node_id))
            .await?;

        Ok(())
    }

    fn determine_round2_vote_for_question(
        &mut self,
        round1_votes: &std::collections::HashMap<NodeId, StateValue>,
    ) -> StateValue {
        // When round 1 is inconclusive, use Rabia's strategy:
        // 1. Count the non-? votes to see if there's a preference
        // 2. If tied or no clear preference, randomize with bias towards V1

        let v0_count = round1_votes
            .values()
            .filter(|&v| *v == StateValue::V0)
            .count();
        let v1_count = round1_votes
            .values()
            .filter(|&v| *v == StateValue::V1)
            .count();

        match v1_count.cmp(&v0_count) {
            std::cmp::Ordering::Greater => {
                // More V1 votes in round 1 - prefer V1
                if self.rng.gen_bool(0.8) {
                    StateValue::V1
                } else {
                    StateValue::V0
                }
            }
            std::cmp::Ordering::Less => {
                // More V0 votes in round 1 - prefer V0
                if self.rng.gen_bool(0.7) {
                    StateValue::V0
                } else {
                    StateValue::V1
                }
            }
            std::cmp::Ordering::Equal => {
                // Tied or no clear preference - bias towards V1 for liveness
                if self.rng.gen_bool(0.6) {
                    StateValue::V1
                } else {
                    StateValue::V0
                }
            }
        }
    }

    async fn handle_vote_round2(&mut self, from: NodeId, vote: VoteRound2Message) -> Result<()> {
        debug!(
            "Received round 2 vote from {} for phase {}",
            from, vote.phase_id
        );

        // Update phase with vote
        self.engine_state.update_phase(vote.phase_id, |phase| {
            phase.add_round2_vote(from, vote.vote);
        })?;

        // Check if we have a decision
        if let Some(phase) = self.engine_state.get_phase(&vote.phase_id) {
            if let Some(decision) = phase.has_round2_majority(self.engine_state.quorum_size) {
                self.make_decision(vote.phase_id, decision).await?;
            }
        }

        Ok(())
    }

    async fn make_decision(&mut self, phase_id: PhaseId, decision: StateValue) -> Result<()> {
        info!("Decision reached for phase {}: {:?}", phase_id, decision);

        // Update phase with decision
        self.engine_state.update_phase(phase_id, |phase| {
            phase.set_decision(decision);
        })?;

        // Apply the batch if decision is V1 (commit)
        if decision == StateValue::V1 {
            if let Some(phase) = self.engine_state.get_phase(&phase_id) {
                if let Some(batch) = &phase.batch {
                    self.apply_batch(batch).await?;
                    if let Err(e) = self.engine_state.commit_phase(phase_id) {
                        error!("Failed to commit phase {}: {}", phase_id, e);
                        return Err(e);
                    }
                }
            }
        }

        // Broadcast decision
        let phase = self.engine_state.get_phase(&phase_id).ok_or_else(|| {
            RabiaError::internal(format!(
                "Phase {} not found for decision broadcast",
                phase_id
            ))
        })?;
        let decision_msg = DecisionMessage {
            phase_id,
            batch_id: phase.batch_id.unwrap_or_default(),
            decision,
            batch: phase.batch,
        };

        let message = ProtocolMessage::decision(self.node_id, decision_msg);
        self.network
            .lock()
            .await
            .broadcast(message, Some(self.node_id))
            .await?;

        Ok(())
    }

    async fn apply_batch(&mut self, batch: &CommandBatch) -> Result<()> {
        debug!(
            "Applying batch {} with {} commands",
            batch.id,
            batch.commands.len()
        );

        // Apply commands without holding the lock for too long
        let results = {
            let mut sm = self.state_machine.lock().await;
            sm.apply_commands(&batch.commands).await?
        }; // Lock is released here

        // Remove from pending batches after successful application
        self.engine_state.remove_pending_batch(&batch.id);

        info!(
            "Successfully applied batch {} with {} results",
            batch.id,
            results.len()
        );
        Ok(())
    }

    async fn handle_decision(&mut self, _from: NodeId, decision: DecisionMessage) -> Result<()> {
        debug!(
            "Received decision for phase {}: {:?}",
            decision.phase_id, decision.decision
        );

        // Update our phase data with the decision
        self.engine_state.update_phase(decision.phase_id, |phase| {
            phase.set_decision(decision.decision);
            if phase.batch.is_none() {
                phase.batch = decision.batch.clone();
            }
        })?;

        // Apply the batch if we haven't already and decision is commit
        if decision.decision == StateValue::V1 {
            if let Some(batch) = &decision.batch {
                // Check if we've already applied this batch
                let last_committed = self.engine_state.last_committed_phase();
                if decision.phase_id > last_committed {
                    self.apply_batch(batch).await?;
                    if let Err(e) = self.engine_state.commit_phase(decision.phase_id) {
                        error!(
                            "Failed to commit phase {} from decision: {}",
                            decision.phase_id, e
                        );
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_sync_request(
        &mut self,
        from: NodeId,
        request: SyncRequestMessage,
    ) -> Result<()> {
        debug!(
            "Received sync request from {} (phase: {})",
            from, request.requester_phase
        );

        // Create sync response with our current state
        let current_phase = self.engine_state.current_phase();
        let state_version = self.engine_state.get_state_version();

        // Create snapshot if we're ahead
        let snapshot = if current_phase > request.requester_phase {
            let sm = self.state_machine.lock().await;
            Some(sm.create_snapshot().await?)
        } else {
            None
        };

        let response = SyncResponseMessage {
            responder_phase: current_phase,
            responder_state_version: state_version,
            state_snapshot: snapshot,
            pending_batches: Vec::new(), // Future enhancement: include pending batches for sync
            committed_phases: Vec::new(), // Future enhancement: include recent committed phases
        };

        let message = ProtocolMessage::sync_response(self.node_id, from, response);
        self.network.lock().await.send_to(from, message).await?;

        Ok(())
    }

    async fn handle_sync_response(
        &mut self,
        from: NodeId,
        response: SyncResponseMessage,
    ) -> Result<()> {
        debug!(
            "Received sync response from {} (phase: {})",
            from, response.responder_phase
        );

        // Store the response for sync resolution
        self.engine_state.add_sync_response(from, response);

        // Check if we have enough responses to proceed with sync
        let sync_responses = self.engine_state.get_sync_responses();
        if sync_responses.len() >= self.engine_state.quorum_size {
            self.resolve_sync(sync_responses).await?;
        }

        Ok(())
    }

    async fn resolve_sync(
        &mut self,
        responses: std::collections::HashMap<NodeId, SyncResponseMessage>,
    ) -> Result<()> {
        info!("Resolving sync with {} responses", responses.len());

        // Find the most recent state among responses
        let latest_response = responses
            .values()
            .max_by_key(|r| r.responder_phase.value())
            .cloned();

        if let Some(latest) = latest_response {
            let current_phase = self.engine_state.current_phase();

            if latest.responder_phase > current_phase {
                info!(
                    "Syncing to phase {} from phase {}",
                    latest.responder_phase, current_phase
                );

                // Update our phase
                self.engine_state.current_phase.store(
                    latest.responder_phase.value(),
                    std::sync::atomic::Ordering::Release,
                );

                // Restore state machine if snapshot provided
                if let Some(snapshot) = latest.state_snapshot {
                    let mut sm = self.state_machine.lock().await;
                    sm.restore_snapshot(&snapshot).await?;
                }
            }
        }

        // Clear sync responses
        self.engine_state.clear_sync_responses();
        Ok(())
    }

    async fn handle_new_batch(&mut self, from: NodeId, new_batch: NewBatchMessage) -> Result<()> {
        debug!("Received new batch from {}", from);

        // Add to pending batches
        self.engine_state
            .add_pending_batch(new_batch.batch, new_batch.originator);

        Ok(())
    }

    async fn handle_heartbeat(
        &mut self,
        _from: NodeId,
        _heartbeat: HeartBeatMessage,
    ) -> Result<()> {
        // Update active nodes based on heartbeat
        // This is a simplified implementation
        Ok(())
    }

    async fn send_heartbeat(&mut self) -> Result<()> {
        let heartbeat = HeartBeatMessage {
            current_phase: self.engine_state.current_phase(),
            last_committed_phase: self.engine_state.last_committed_phase(),
            active: self.engine_state.is_active(),
        };

        let message = ProtocolMessage::new(self.node_id, None, MessageType::HeartBeat(heartbeat));

        self.network
            .lock()
            .await
            .broadcast(message, Some(self.node_id))
            .await?;
        Ok(())
    }

    async fn advance_to_next_phase(&mut self) -> Result<()> {
        let new_phase = self.engine_state.advance_phase();
        info!("Advanced to phase {}", new_phase);
        Ok(())
    }

    async fn initiate_sync(&mut self) -> Result<()> {
        info!("Initiating synchronization");

        let request = SyncRequestMessage {
            requester_phase: self.engine_state.current_phase(),
            requester_state_version: self.engine_state.get_state_version(),
        };

        // Send sync request to all active nodes
        let active_nodes = self.engine_state.get_active_nodes();
        for node_id in active_nodes {
            if node_id != self.node_id {
                let message = ProtocolMessage::sync_request(self.node_id, node_id, request.clone());
                self.network.lock().await.send_to(node_id, message).await?;
            }
        }

        Ok(())
    }

    async fn cleanup_old_state(&mut self) {
        let removed_phases = self
            .engine_state
            .cleanup_old_phases(self.config.max_phase_history);
        let removed_batches = self.engine_state.cleanup_old_pending_batches(300); // 5 minutes

        if removed_phases > 0 || removed_batches > 0 {
            debug!(
                "Cleaned up {} old phases and {} old batches",
                removed_phases, removed_batches
            );
        }
    }

    async fn receive_messages(&self, buffer: &mut Vec<(NodeId, ProtocolMessage)>) -> Result<()> {
        // Try to receive multiple messages in a batch for efficiency
        let mut network = self.network.lock().await;

        match timeout(Duration::from_millis(10), network.receive()).await {
            Ok(Ok((from, message))) => {
                buffer.push((from, message));

                // Try to get more messages without blocking
                for _ in 0..10 {
                    // Limit to prevent starvation
                    match timeout(Duration::from_millis(1), network.receive()).await {
                        Ok(Ok((from, message))) => buffer.push((from, message)),
                        _ => break,
                    }
                }
            }
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                // Timeout - no messages available
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl<SM, NT, PL> NetworkEventHandler for RabiaEngine<SM, NT, PL>
where
    SM: StateMachine + 'static,
    NT: NetworkTransport + 'static,
    PL: PersistenceLayer + 'static,
{
    async fn on_node_connected(&self, node_id: NodeId) {
        info!("Node {} connected", node_id);
    }

    async fn on_node_disconnected(&self, node_id: NodeId) {
        warn!("Node {} disconnected", node_id);
    }

    async fn on_network_partition(&self, active_nodes: HashSet<NodeId>) {
        warn!(
            "Network partition detected, {} active nodes",
            active_nodes.len()
        );
        self.engine_state.update_active_nodes(active_nodes);
    }

    async fn on_quorum_lost(&self) {
        error!("Quorum lost - stopping consensus operations");
        self.engine_state.set_active(false);
    }

    async fn on_quorum_restored(&self, active_nodes: HashSet<NodeId>) {
        info!("Quorum restored with {} nodes", active_nodes.len());
        self.engine_state.update_active_nodes(active_nodes);
        self.engine_state.set_active(true);
    }
}
