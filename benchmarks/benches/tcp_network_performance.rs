//! TCP Network Performance Benchmarks
//!
//! These benchmarks test the real TCP network implementation performance
//! with actual network connections and measure throughput, latency, and scalability.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::time::sleep;
use tracing::{info, warn};

use rabia_core::{
    messages::{HeartBeatMessage, MessageType, ProposeMessage, ProtocolMessage},
    network::NetworkTransport,
    BatchId, Command, CommandBatch, NodeId, PhaseId, StateValue,
};
use rabia_network::{TcpNetwork, TcpNetworkConfig};

/// Benchmark configuration for network tests
#[derive(Clone)]
struct NetworkBenchConfig {
    node_count: usize,
    message_count: usize,
    message_size: usize,
    concurrent_senders: usize,
}

impl Default for NetworkBenchConfig {
    fn default() -> Self {
        Self {
            node_count: 3,
            message_count: 100,
            message_size: 1024,
            concurrent_senders: 1,
        }
    }
}

/// Create a test cluster with TCP networking
async fn create_tcp_cluster(config: &NetworkBenchConfig) -> Vec<TcpNetwork> {
    let mut networks = Vec::new();
    let mut node_addresses = HashMap::new();

    // Create all nodes first
    for i in 0..config.node_count {
        let node_id = NodeId::new();
        let net_config = TcpNetworkConfig {
            bind_addr: format!("127.0.0.1:{}", 20000 + i).parse().unwrap(),
            ..Default::default()
        };

        let network = TcpNetwork::new(node_id, net_config).await.unwrap();
        let addr = network.local_addr();
        node_addresses.insert(node_id, addr);
        networks.push(network);
    }

    // Connect all nodes to each other (full mesh)
    for i in 0..networks.len() {
        for j in 0..networks.len() {
            if i != j {
                let target_node = networks[j].node_id;
                let target_addr = node_addresses[&target_node];

                if let Err(e) = networks[i].connect_to_peer(target_node, target_addr).await {
                    warn!("Failed to connect node {} to {}: {}", i, j, e);
                }
            }
        }
    }

    // Wait for connections to establish
    sleep(Duration::from_millis(500)).await;

    networks
}

/// Benchmark point-to-point message throughput
fn benchmark_p2p_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let configs = vec![
        NetworkBenchConfig {
            node_count: 2,
            message_count: 100,
            ..Default::default()
        },
        NetworkBenchConfig {
            node_count: 2,
            message_count: 500,
            ..Default::default()
        },
        NetworkBenchConfig {
            node_count: 2,
            message_count: 1000,
            ..Default::default()
        },
    ];

    for config in configs {
        c.bench_with_input(
            BenchmarkId::new("tcp_p2p_throughput", config.message_count),
            &config,
            |b, config| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let mut total_duration = Duration::ZERO;

                    for _ in 0..iters {
                        let networks = create_tcp_cluster(config).await;
                        let mut sender = networks[0].clone();
                        let mut receiver = networks[1].clone();

                        let target_node = receiver.node_id;

                        let start = Instant::now();

                        // Send messages
                        for i in 0..config.message_count {
                            let message = ProtocolMessage::new(
                                sender.node_id,
                                Some(target_node),
                                MessageType::HeartBeat(HeartBeatMessage {
                                    current_phase: PhaseId::new(i as u64),
                                    last_committed_phase: PhaseId::new(0),
                                    active: true,
                                }),
                            );

                            black_box(sender.send_to(target_node, message).await.unwrap());
                        }

                        // Receive messages
                        for _ in 0..config.message_count {
                            match tokio::time::timeout(Duration::from_secs(10), receiver.receive())
                                .await
                            {
                                Ok(Ok(_)) => {}
                                _ => break,
                            }
                        }

                        total_duration += start.elapsed();

                        // Cleanup
                        for network in networks {
                            network.shutdown().await;
                        }
                        sleep(Duration::from_millis(50)).await;
                    }

                    total_duration
                });
            },
        );
    }
}

/// Benchmark broadcast performance
fn benchmark_broadcast_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let configs = vec![
        NetworkBenchConfig {
            node_count: 3,
            message_count: 50,
            ..Default::default()
        },
        NetworkBenchConfig {
            node_count: 5,
            message_count: 50,
            ..Default::default()
        },
        NetworkBenchConfig {
            node_count: 7,
            message_count: 50,
            ..Default::default()
        },
    ];

    for config in configs {
        c.bench_with_input(
            BenchmarkId::new("tcp_broadcast", config.node_count),
            &config,
            |b, config| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let mut total_duration = Duration::ZERO;

                    for _ in 0..iters {
                        let networks = create_tcp_cluster(config).await;
                        let broadcaster = networks[0].clone();
                        let mut receivers: Vec<_> = networks[1..].iter().cloned().collect();

                        let start = Instant::now();

                        // Send broadcasts
                        for i in 0..config.message_count {
                            let message = ProtocolMessage::new(
                                broadcaster.node_id,
                                None,
                                MessageType::HeartBeat(HeartBeatMessage {
                                    current_phase: PhaseId::new(i as u64),
                                    last_committed_phase: PhaseId::new(0),
                                    active: true,
                                }),
                            );

                            black_box(
                                broadcaster
                                    .broadcast(message, Some(broadcaster.node_id))
                                    .await
                                    .unwrap(),
                            );
                        }

                        // Receive on all nodes
                        let expected_total = config.message_count * (config.node_count - 1);
                        let mut received_total = 0;

                        let timeout_duration = Duration::from_secs(10);
                        let deadline = Instant::now() + timeout_duration;

                        while received_total < expected_total && Instant::now() < deadline {
                            for receiver in &mut receivers {
                                match tokio::time::timeout(
                                    Duration::from_millis(10),
                                    receiver.receive(),
                                )
                                .await
                                {
                                    Ok(Ok(_)) => received_total += 1,
                                    _ => {}
                                }
                            }
                        }

                        total_duration += start.elapsed();

                        // Cleanup
                        for network in networks {
                            network.shutdown().await;
                        }
                        sleep(Duration::from_millis(100)).await;
                    }

                    total_duration
                });
            },
        );
    }
}

/// Benchmark large message handling
fn benchmark_large_messages(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let message_sizes = vec![1024, 8192, 65536]; // 1KB, 8KB, 64KB

    for size in message_sizes {
        c.bench_with_input(
            BenchmarkId::new("tcp_large_messages", size),
            &size,
            |b, &message_size| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let mut total_duration = Duration::ZERO;

                    for _ in 0..iters {
                        let config = NetworkBenchConfig {
                            node_count: 2,
                            message_count: 10,
                            message_size,
                            ..Default::default()
                        };

                        let networks = create_tcp_cluster(&config).await;
                        let mut sender = networks[0].clone();
                        let mut receiver = networks[1].clone();

                        let target_node = receiver.node_id;

                        // Create large command batch
                        let commands: Vec<Command> = (0..message_size / 50)
                            .map(|i| Command::new(format!("SET key{} {}", i, "x".repeat(30))))
                            .collect();
                        let batch = CommandBatch::new(commands);

                        let start = Instant::now();

                        // Send large messages
                        for i in 0..config.message_count {
                            let message = ProtocolMessage::new(
                                sender.node_id,
                                Some(target_node),
                                MessageType::Propose(ProposeMessage {
                                    phase_id: PhaseId::new(i as u64),
                                    batch_id: BatchId::new(),
                                    value: StateValue::V1,
                                    batch: Some(batch.clone()),
                                }),
                            );

                            black_box(sender.send_to(target_node, message).await.unwrap());
                        }

                        // Receive messages
                        for _ in 0..config.message_count {
                            match tokio::time::timeout(Duration::from_secs(30), receiver.receive())
                                .await
                            {
                                Ok(Ok(_)) => {}
                                _ => break,
                            }
                        }

                        total_duration += start.elapsed();

                        // Cleanup
                        for network in networks {
                            network.shutdown().await;
                        }
                    }

                    total_duration
                });
            },
        );
    }
}

/// Benchmark concurrent connections
fn benchmark_concurrent_connections(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let concurrent_counts = vec![5, 10, 20];

    for concurrent in concurrent_counts {
        c.bench_with_input(
            BenchmarkId::new("tcp_concurrent", concurrent),
            &concurrent,
            |b, &concurrent_senders| {
                b.to_async(&rt).iter_custom(|iters| async move {
                    let mut total_duration = Duration::ZERO;

                    for _ in 0..iters {
                        let config = NetworkBenchConfig {
                            node_count: concurrent_senders + 1,
                            message_count: 20,
                            concurrent_senders,
                            ..Default::default()
                        };

                        let networks = create_tcp_cluster(&config).await;
                        let receiver_network = networks.last().unwrap().clone();
                        let mut receiver = receiver_network.clone();
                        let sender_networks: Vec<_> =
                            networks[..concurrent_senders].iter().cloned().collect();

                        let start = Instant::now();

                        // Spawn concurrent senders
                        let mut handles = Vec::new();
                        for (i, sender_network) in sender_networks.iter().enumerate() {
                            let sender = sender_network.clone();
                            let target_node = receiver_network.node_id;
                            let messages_per_sender = config.message_count / concurrent_senders;

                            let handle = tokio::spawn(async move {
                                for j in 0..messages_per_sender {
                                    let message = ProtocolMessage::new(
                                        sender.node_id,
                                        Some(target_node),
                                        MessageType::HeartBeat(HeartBeatMessage {
                                            current_phase: PhaseId::new((i * 1000 + j) as u64),
                                            last_committed_phase: PhaseId::new(0),
                                            active: true,
                                        }),
                                    );

                                    sender.send_to(target_node, message).await.unwrap();
                                }
                            });
                            handles.push(handle);
                        }

                        // Wait for all senders to complete
                        for handle in handles {
                            handle.await.unwrap();
                        }

                        // Receive all messages
                        let expected_messages =
                            (config.message_count / concurrent_senders) * concurrent_senders;
                        for _ in 0..expected_messages {
                            match tokio::time::timeout(Duration::from_secs(10), receiver.receive())
                                .await
                            {
                                Ok(Ok(_)) => {}
                                _ => break,
                            }
                        }

                        total_duration += start.elapsed();

                        // Cleanup
                        for network in networks {
                            network.shutdown().await;
                        }
                        sleep(Duration::from_millis(100)).await;
                    }

                    total_duration
                });
            },
        );
    }
}

/// Benchmark network latency
fn benchmark_network_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("tcp_round_trip_latency", |b| {
        b.to_async(&rt).iter_custom(|iters| async move {
            let mut total_duration = Duration::ZERO;

            for _ in 0..iters {
                let config = NetworkBenchConfig {
                    node_count: 2,
                    message_count: 1,
                    ..Default::default()
                };

                let networks = create_tcp_cluster(&config).await;
                let mut sender = networks[0].clone();
                let mut receiver = networks[1].clone();

                let target_node = receiver.node_id;

                let start = Instant::now();

                // Send one message and measure round-trip
                let message = ProtocolMessage::new(
                    sender.node_id,
                    Some(target_node),
                    MessageType::HeartBeat(HeartBeatMessage {
                        current_phase: PhaseId::new(1),
                        last_committed_phase: PhaseId::new(0),
                        active: true,
                    }),
                );

                black_box(sender.send_to(target_node, message).await.unwrap());

                // Receive the message
                match tokio::time::timeout(Duration::from_secs(5), receiver.receive()).await {
                    Ok(Ok(_)) => {}
                    _ => {}
                }

                total_duration += start.elapsed();

                // Cleanup
                for network in networks {
                    network.shutdown().await;
                }
            }

            total_duration
        });
    });
}

criterion_group!(
    benches,
    benchmark_p2p_throughput,
    benchmark_broadcast_performance,
    benchmark_large_messages,
    benchmark_concurrent_connections,
    benchmark_network_latency
);
criterion_main!(benches);
