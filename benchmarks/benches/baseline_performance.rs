use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rabia_core::{messages::*, Command, CommandBatch, PhaseId};

fn benchmark_message_serialization(c: &mut Criterion) {
    let propose_msg = ProtocolMessage::new(
        rabia_core::NodeId::new(),
        Some(rabia_core::NodeId::new()),
        MessageType::Propose(ProposeMessage {
            phase_id: PhaseId::new(1),
            batch_id: rabia_core::BatchId::new(),
            value: rabia_core::StateValue::V1,
            batch: Some(CommandBatch::new(vec![
                Command::new("SET key1 value1"),
                Command::new("SET key2 value2"),
                Command::new("GET key1"),
            ])),
        }),
    );

    c.bench_function("json_serialize_propose", |b| {
        b.iter(|| serde_json::to_string(black_box(&propose_msg)))
    });

    c.bench_function("json_deserialize_propose", |b| {
        let serialized = serde_json::to_string(&propose_msg).unwrap();
        b.iter(|| {
            let _: ProtocolMessage = serde_json::from_str(black_box(&serialized)).unwrap();
        })
    });
}

fn benchmark_command_batch_creation(c: &mut Criterion) {
    let commands: Vec<Command> = (0..100)
        .map(|i| Command::new(format!("SET key{} value{}", i, i)))
        .collect();

    c.bench_function("batch_creation_100_commands", |b| {
        b.iter(|| CommandBatch::new(black_box(commands.clone())))
    });

    c.bench_function("batch_validation_100_commands", |b| {
        let batch = CommandBatch::new(commands.clone());
        b.iter(|| {
            use rabia_core::validation::Validator;
            batch.validate()
        })
    });
}

fn benchmark_memory_allocations(c: &mut Criterion) {
    c.bench_function("node_id_creation", |b| b.iter(rabia_core::NodeId::new));

    c.bench_function("phase_id_creation", |b| {
        b.iter(|| PhaseId::new(black_box(12345)))
    });

    c.bench_function("command_creation", |b| {
        b.iter(|| Command::new(black_box("SET test_key test_value")))
    });
}

criterion_group!(
    benches,
    benchmark_message_serialization,
    benchmark_command_batch_creation,
    benchmark_memory_allocations
);
criterion_main!(benches);
