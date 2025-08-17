use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use rabia_core::{Command, CommandBatch, messages::*, PhaseId, NodeId, StateValue, BatchId};
use rabia_core::serialization::Serializer;
use rabia_core::batching::{CommandBatcher, BatchConfig, BatchProcessor};
use std::time::Duration;

fn create_test_commands(count: usize) -> Vec<Command> {
    (0..count)
        .map(|i| Command::new(format!("SET key{} value{}", i, i)))
        .collect()
}

fn create_test_message_with_batch(commands: Vec<Command>) -> ProtocolMessage {
    ProtocolMessage::new(
        NodeId::new(),
        Some(NodeId::new()),
        MessageType::Propose(ProposeMessage {
            phase_id: PhaseId::new(1),
            batch_id: BatchId::new(),
            value: StateValue::V1,
            batch: Some(CommandBatch::new(commands)),
        }),
    )
}

fn benchmark_end_to_end_pipeline(c: &mut Criterion) {
    // Compare the complete pipeline: individual vs batched + binary serialization
    
    c.bench_function("baseline_individual_processing", |b| {
        b.iter_batched(
            || create_test_commands(50),
            |commands| {
                let json_serializer = Serializer::json();
                let mut results = Vec::new();
                
                // Process commands individually (baseline)
                for command in commands {
                    let single_batch = CommandBatch::new(vec![command]);
                    let message = create_test_message_with_batch(single_batch.commands.clone());
                    let serialized = json_serializer.serialize_message(black_box(&message)).unwrap();
                    results.push(serialized);
                }
                results
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("optimized_batched_processing", |b| {
        b.iter_batched(
            || create_test_commands(50),
            |commands| {
                let binary_serializer = Serializer::binary();
                let mut batcher = CommandBatcher::new(BatchConfig {
                    max_batch_size: 10,
                    max_batch_delay: Duration::from_millis(1),
                    buffer_capacity: 100,
                    adaptive: false,
                });
                
                let mut results = Vec::new();
                
                // Process commands in batches
                for command in commands {
                    if let Some(batch) = batcher.add_command(command).unwrap() {
                        let message = create_test_message_with_batch(batch.commands.clone());
                        let serialized = binary_serializer.serialize_message(black_box(&message)).unwrap();
                        results.push(serialized);
                    }
                }
                
                // Flush remaining
                if let Some(batch) = batcher.flush() {
                    let message = create_test_message_with_batch(batch.commands.clone());
                    let serialized = binary_serializer.serialize_message(&message).unwrap();
                    results.push(serialized);
                }
                
                results
            },
            BatchSize::SmallInput,
        )
    });
}

fn benchmark_serialization_with_batching(c: &mut Criterion) {
    let small_batch = create_test_commands(10);
    let large_batch = create_test_commands(100);
    
    // JSON vs Binary with different batch sizes
    c.bench_function("json_small_batch", |b| {
        b.iter(|| {
            let serializer = Serializer::json();
            let message = create_test_message_with_batch(small_batch.clone());
            serializer.serialize_message(black_box(&message)).unwrap()
        })
    });

    c.bench_function("binary_small_batch", |b| {
        b.iter(|| {
            let serializer = Serializer::binary();
            let message = create_test_message_with_batch(small_batch.clone());
            serializer.serialize_message(black_box(&message)).unwrap()
        })
    });

    c.bench_function("json_large_batch", |b| {
        b.iter(|| {
            let serializer = Serializer::json();
            let message = create_test_message_with_batch(large_batch.clone());
            serializer.serialize_message(black_box(&message)).unwrap()
        })
    });

    c.bench_function("binary_large_batch", |b| {
        b.iter(|| {
            let serializer = Serializer::binary();
            let message = create_test_message_with_batch(large_batch.clone());
            serializer.serialize_message(black_box(&message)).unwrap()
        })
    });
}

fn benchmark_adaptive_batching(c: &mut Criterion) {
    // Test different batching strategies
    
    c.bench_function("fixed_batching_size_10", |b| {
        b.iter_batched(
            || create_test_commands(100),
            |commands| {
                let mut batcher = CommandBatcher::new(BatchConfig {
                    max_batch_size: 10,
                    max_batch_delay: Duration::from_millis(1),
                    buffer_capacity: 200,
                    adaptive: false,
                });
                
                let mut batch_count = 0;
                for command in commands {
                    if let Some(_batch) = batcher.add_command(command).unwrap() {
                        batch_count += 1;
                    }
                }
                if batcher.flush().is_some() {
                    batch_count += 1;
                }
                batch_count
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("adaptive_batching", |b| {
        b.iter_batched(
            || create_test_commands(100),
            |commands| {
                let mut batcher = CommandBatcher::new(BatchConfig {
                    max_batch_size: 20,
                    max_batch_delay: Duration::from_millis(1),
                    buffer_capacity: 200,
                    adaptive: true,
                });
                
                let mut batch_count = 0;
                for command in commands {
                    if let Some(_batch) = batcher.add_command(command).unwrap() {
                        batch_count += 1;
                    }
                }
                if batcher.flush().is_some() {
                    batch_count += 1;
                }
                batch_count
            },
            BatchSize::SmallInput,
        )
    });
}

fn benchmark_throughput_comparison(c: &mut Criterion) {
    // Simulate high-throughput scenario
    
    c.bench_function("throughput_baseline", |b| {
        b.iter(|| {
            let json_serializer = Serializer::json();
            let mut total_bytes = 0;
            
            for i in 0..1000 {
                let command = Command::new(format!("SET key{} value{}", i, i));
                let batch = CommandBatch::new(vec![command]);
                let message = create_test_message_with_batch(batch.commands.clone());
                let serialized = json_serializer.serialize_message(&message).unwrap();
                total_bytes += serialized.len();
            }
            total_bytes
        })
    });

    c.bench_function("throughput_optimized", |b| {
        b.iter(|| {
            let binary_serializer = Serializer::binary();
            let processor = BatchProcessor::new();
            let mut batcher = CommandBatcher::new(BatchConfig {
                max_batch_size: 50,
                max_batch_delay: Duration::from_millis(1),
                buffer_capacity: 1000,
                adaptive: true,
            });
            
            let mut total_bytes = 0;
            
            for i in 0..1000 {
                let command = Command::new(format!("SET key{} value{}", i, i));
                
                if let Some(batch) = batcher.add_command(command).unwrap() {
                    let message = create_test_message_with_batch(batch.commands.clone());
                    let serialized = binary_serializer.serialize_message(&message).unwrap();
                    total_bytes += serialized.len();
                }
            }
            
            // Process remaining
            if let Some(batch) = batcher.flush() {
                let message = create_test_message_with_batch(batch.commands.clone());
                let serialized = binary_serializer.serialize_message(&message).unwrap();
                total_bytes += serialized.len();
            }
            
            total_bytes
        })
    });
}

fn benchmark_memory_efficiency(c: &mut Criterion) {
    // Test memory allocation patterns
    
    c.bench_function("memory_baseline_many_small", |b| {
        b.iter(|| {
            let serializer = Serializer::json();
            let mut results = Vec::new();
            
            for i in 0..100 {
                let command = Command::new(format!("SET key{} value{}", i, i));
                let batch = CommandBatch::new(vec![command]);
                let message = create_test_message_with_batch(batch.commands.clone());
                let serialized = serializer.serialize_message(&message).unwrap();
                results.push(serialized);
            }
            results.len()
        })
    });

    c.bench_function("memory_optimized_batched", |b| {
        b.iter(|| {
            let serializer = Serializer::binary();
            let mut batcher = CommandBatcher::new(BatchConfig::default());
            let mut batch_count = 0;
            
            for i in 0..100 {
                let command = Command::new(format!("SET key{} value{}", i, i));
                
                if let Some(batch) = batcher.add_command(command).unwrap() {
                    let message = create_test_message_with_batch(batch.commands.clone());
                    let _serialized = serializer.serialize_message(&message).unwrap();
                    batch_count += 1;
                }
            }
            
            if let Some(batch) = batcher.flush() {
                let message = create_test_message_with_batch(batch.commands.clone());
                let _serialized = serializer.serialize_message(&message).unwrap();
                batch_count += 1;
            }
            
            batch_count
        })
    });
}

criterion_group!(
    benches,
    benchmark_end_to_end_pipeline,
    benchmark_serialization_with_batching,
    benchmark_adaptive_batching,
    benchmark_throughput_comparison,
    benchmark_memory_efficiency
);
criterion_main!(benches);