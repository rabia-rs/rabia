use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rabia_core::{Command, CommandBatch, messages::*, PhaseId, NodeId, StateValue, BatchId};
use rabia_core::serialization::{Serializer, MessageSerializer};

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

fn create_large_message() -> ProtocolMessage {
    let commands: Vec<Command> = (0..100)
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
}

fn benchmark_serialization_comparison(c: &mut Criterion) {
    let json_serializer = Serializer::json();
    let binary_serializer = Serializer::binary();
    let message = create_test_message();
    let large_message = create_large_message();

    // Small message serialization
    c.bench_function("json_serialize_small", |b| {
        b.iter(|| json_serializer.serialize(black_box(&message)))
    });

    c.bench_function("binary_serialize_small", |b| {
        b.iter(|| binary_serializer.serialize(black_box(&message)))
    });

    // Small message deserialization
    let json_serialized = json_serializer.serialize(&message).unwrap();
    let binary_serialized = binary_serializer.serialize(&message).unwrap();

    c.bench_function("json_deserialize_small", |b| {
        b.iter(|| {
            let _: ProtocolMessage = json_serializer.deserialize(black_box(&json_serialized)).unwrap();
        })
    });

    c.bench_function("binary_deserialize_small", |b| {
        b.iter(|| {
            let _: ProtocolMessage = binary_serializer.deserialize(black_box(&binary_serialized)).unwrap();
        })
    });

    // Large message serialization
    c.bench_function("json_serialize_large", |b| {
        b.iter(|| json_serializer.serialize(black_box(&large_message)))
    });

    c.bench_function("binary_serialize_large", |b| {
        b.iter(|| binary_serializer.serialize(black_box(&large_message)))
    });

    // Large message deserialization
    let json_large_serialized = json_serializer.serialize(&large_message).unwrap();
    let binary_large_serialized = binary_serializer.serialize(&large_message).unwrap();

    c.bench_function("json_deserialize_large", |b| {
        b.iter(|| {
            let _: ProtocolMessage = json_serializer.deserialize(black_box(&json_large_serialized)).unwrap();
        })
    });

    c.bench_function("binary_deserialize_large", |b| {
        b.iter(|| {
            let _: ProtocolMessage = binary_serializer.deserialize(black_box(&binary_large_serialized)).unwrap();
        })
    });
}

fn benchmark_message_sizes(c: &mut Criterion) {
    let json_serializer = Serializer::json();
    let binary_serializer = Serializer::binary();

    c.bench_function("size_comparison_small", |b| {
        b.iter(|| {
            let message = create_test_message();
            let json_size = json_serializer.serialize(black_box(&message)).unwrap().len();
            let binary_size = binary_serializer.serialize(black_box(&message)).unwrap().len();
            (json_size, binary_size)
        })
    });

    c.bench_function("size_comparison_large", |b| {
        b.iter(|| {
            let message = create_large_message();
            let json_size = json_serializer.serialize(black_box(&message)).unwrap().len();
            let binary_size = binary_serializer.serialize(black_box(&message)).unwrap().len();
            (json_size, binary_size)
        })
    });
}

fn benchmark_roundtrip_performance(c: &mut Criterion) {
    let json_serializer = Serializer::json();
    let binary_serializer = Serializer::binary();
    let message = create_test_message();

    c.bench_function("json_roundtrip", |b| {
        b.iter(|| {
            let serialized = json_serializer.serialize(black_box(&message)).unwrap();
            let _: ProtocolMessage = json_serializer.deserialize(&serialized).unwrap();
        })
    });

    c.bench_function("binary_roundtrip", |b| {
        b.iter(|| {
            let serialized = binary_serializer.serialize(black_box(&message)).unwrap();
            let _: ProtocolMessage = binary_serializer.deserialize(&serialized).unwrap();
        })
    });
}

criterion_group!(
    benches,
    benchmark_serialization_comparison,
    benchmark_message_sizes,
    benchmark_roundtrip_performance
);
criterion_main!(benches);