use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use rabia_core::memory_pool::{get_pooled_buffer, MemoryPool};
use rabia_core::serialization::Serializer;
use rabia_core::{messages::*, BatchId, Command, CommandBatch, NodeId, PhaseId, StateValue};

fn create_test_message() -> ProtocolMessage {
    ProtocolMessage::new(
        NodeId::new(),
        Some(NodeId::new()),
        MessageType::Propose(ProposeMessage {
            phase_id: PhaseId::new(1),
            batch_id: BatchId::new(),
            value: StateValue::V1,
            batch: Some(CommandBatch::new(vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
                Command::new("GET key1"),
                Command::new("SET key3 value3"),
                Command::new("DELETE key2"),
            ])),
        }),
    )
}

fn benchmark_memory_allocation_comparison(c: &mut Criterion) {
    let serializer = Serializer::binary();

    // Benchmark regular serialization (allocates new Vec each time)
    c.bench_function("regular_serialization", |b| {
        b.iter_batched(
            create_test_message,
            |message| {
                let _serialized = serializer.serialize_message(black_box(&message)).unwrap();
            },
            BatchSize::SmallInput,
        )
    });

    // Benchmark pooled serialization (reuses buffers)
    c.bench_function("pooled_serialization", |b| {
        b.iter_batched(
            create_test_message,
            |message| {
                let _pooled_buffer = serializer
                    .serialize_message_pooled(black_box(&message))
                    .unwrap();
            },
            BatchSize::SmallInput,
        )
    });
}

fn benchmark_buffer_reuse(c: &mut Criterion) {
    let pool = MemoryPool::default();
    pool.warm_up(); // Pre-warm the pool

    // Benchmark manual buffer allocation
    c.bench_function("manual_buffer_allocation", |b| {
        b.iter(|| {
            let mut buffer = Vec::with_capacity(1024);
            buffer.extend_from_slice(black_box(b"Hello, World! This is a test message."));
            buffer
        })
    });

    // Benchmark pooled buffer allocation
    c.bench_function("pooled_buffer_allocation", |b| {
        b.iter(|| {
            let mut buffer = get_pooled_buffer(1024);
            buffer
                .buffer_mut()
                .extend_from_slice(black_box(b"Hello, World! This is a test message."));
            buffer
        })
    });
}

fn benchmark_high_frequency_allocations(c: &mut Criterion) {
    let serializer = Serializer::binary();

    // Simulate high-frequency message processing
    c.bench_function("high_freq_regular", |b| {
        b.iter(|| {
            for i in 0..100 {
                let message = create_test_message();
                let _serialized = serializer.serialize_message(black_box(&message)).unwrap();
                std::hint::black_box(i);
            }
        })
    });

    c.bench_function("high_freq_pooled", |b| {
        b.iter(|| {
            for i in 0..100 {
                let message = create_test_message();
                let _pooled = serializer
                    .serialize_message_pooled(black_box(&message))
                    .unwrap();
                std::hint::black_box(i);
            }
        })
    });
}

fn benchmark_memory_pressure(c: &mut Criterion) {
    // Create larger messages to increase memory pressure
    let create_large_message = || {
        let commands: Vec<Command> = (0..50)
            .map(|i| Command::new(format!("SET key{} value{}", i, i)))
            .collect();

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
    };

    let serializer = Serializer::binary();

    c.bench_function("large_msg_regular", |b| {
        b.iter_batched(
            create_large_message,
            |message| {
                let _serialized = serializer.serialize_message(black_box(&message)).unwrap();
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("large_msg_pooled", |b| {
        b.iter_batched(
            create_large_message,
            |message| {
                let _pooled = serializer
                    .serialize_message_pooled(black_box(&message))
                    .unwrap();
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    benchmark_memory_allocation_comparison,
    benchmark_buffer_reuse,
    benchmark_high_frequency_allocations,
    benchmark_memory_pressure
);
criterion_main!(benches);
