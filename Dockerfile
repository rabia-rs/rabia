# Multi-stage Dockerfile for Rabia consensus examples
FROM rust:1.70-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /usr/src/rabia

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./

# Copy all crates
COPY rabia-core/ ./rabia-core/
COPY rabia-engine/ ./rabia-engine/
COPY rabia-network/ ./rabia-network/
COPY rabia-persistence/ ./rabia-persistence/
COPY rabia-testing/ ./rabia-testing/
COPY rabia-kvstore/ ./rabia-kvstore/
COPY examples/ ./examples/
COPY benchmarks/ ./benchmarks/

# Build the project in release mode
RUN cargo build --release --examples

# Runtime stage
FROM debian:bookworm-slim as runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false rabia

# Copy built examples
COPY --from=builder /usr/src/rabia/target/release/basic_usage /usr/local/bin/
COPY --from=builder /usr/src/rabia/target/release/kvstore_usage /usr/local/bin/
COPY --from=builder /usr/src/rabia/target/release/consensus_cluster /usr/local/bin/
COPY --from=builder /usr/src/rabia/target/release/performance_benchmark /usr/local/bin/

# Copy documentation
COPY README.md LICENSE API_DOCUMENTATION.md /usr/share/doc/rabia/

# Set up directories
RUN mkdir -p /var/lib/rabia /var/log/rabia && \
    chown rabia:rabia /var/lib/rabia /var/log/rabia

# Switch to non-root user
USER rabia

# Set working directory for data
WORKDIR /var/lib/rabia

# Default to running the KVStore example
CMD ["kvstore_usage"]

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD pgrep -f kvstore_usage > /dev/null || exit 1

# Labels
LABEL maintainer="Rabia Contributors"
LABEL description="Rabia consensus protocol examples"
LABEL version="0.2.0"
LABEL org.opencontainers.image.source="https://github.com/rabia-rs/rabia"
LABEL org.opencontainers.image.description="High-performance Rust implementation of the Rabia consensus protocol"
LABEL org.opencontainers.image.licenses="Apache-2.0"