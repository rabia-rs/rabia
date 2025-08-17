use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;
use crate::{Command, CommandBatch, Result, RabiaError};

/// Configuration for batching behavior
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of commands per batch
    pub max_batch_size: usize,
    /// Maximum time to wait before flushing a partial batch
    pub max_batch_delay: Duration,
    /// Buffer size for incoming commands
    pub buffer_capacity: usize,
    /// Enable adaptive batching based on load
    pub adaptive: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_batch_delay: Duration::from_millis(10),
            buffer_capacity: 1000,
            adaptive: true,
        }
    }
}

/// Statistics for batch processing
#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    pub total_commands: usize,
    pub total_batches: usize,
    pub average_batch_size: f64,
    pub commands_dropped: usize,
    pub flush_timeouts: usize,
    pub adaptive_adjustments: usize,
}

impl BatchStats {
    pub fn record_batch(&mut self, batch_size: usize) {
        self.total_commands += batch_size;
        self.total_batches += 1;
        self.average_batch_size = self.total_commands as f64 / self.total_batches as f64;
    }
}

/// A high-performance command batcher that groups commands for efficient processing
pub struct CommandBatcher {
    config: BatchConfig,
    buffer: VecDeque<Command>,
    stats: BatchStats,
    last_flush: Instant,
    adaptive_batch_size: usize,
}

impl CommandBatcher {
    pub fn new(config: BatchConfig) -> Self {
        let adaptive_batch_size = config.max_batch_size;
        let buffer_capacity = config.buffer_capacity;
        Self {
            config,
            buffer: VecDeque::with_capacity(buffer_capacity),
            stats: BatchStats::default(),
            last_flush: Instant::now(),
            adaptive_batch_size,
        }
    }

    /// Add a command to the batch buffer
    pub fn add_command(&mut self, command: Command) -> Result<Option<CommandBatch>> {
        // Check if buffer is full
        if self.buffer.len() >= self.config.buffer_capacity {
            self.stats.commands_dropped += 1;
            return Err(RabiaError::internal("Command buffer overflow"));
        }

        self.buffer.push_back(command);

        // Check if we should flush based on size
        if self.buffer.len() >= self.current_batch_size() {
            return Ok(Some(self.flush_batch()));
        }

        // Check if we should flush based on time
        if self.last_flush.elapsed() >= self.config.max_batch_delay && !self.buffer.is_empty() {
            self.stats.flush_timeouts += 1;
            return Ok(Some(self.flush_batch()));
        }

        Ok(None)
    }

    /// Force flush current batch regardless of size or time
    pub fn flush(&mut self) -> Option<CommandBatch> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(self.flush_batch())
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> &BatchStats {
        &self.stats
    }

    /// Update configuration (for runtime tuning)
    pub fn update_config(&mut self, config: BatchConfig) {
        self.adaptive_batch_size = config.max_batch_size;
        self.config = config;
    }

    /// Get current buffer length
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    fn flush_batch(&mut self) -> CommandBatch {
        let batch_size = self.current_batch_size().min(self.buffer.len());
        let commands: Vec<Command> = self.buffer.drain(..batch_size).collect();
        
        let batch = CommandBatch::new(commands);
        self.stats.record_batch(batch.commands.len());
        self.last_flush = Instant::now();
        
        // Adaptive batching: adjust batch size based on performance
        if self.config.adaptive {
            self.adjust_adaptive_batch_size();
        }
        
        batch
    }

    fn current_batch_size(&self) -> usize {
        if self.config.adaptive {
            self.adaptive_batch_size
        } else {
            self.config.max_batch_size
        }
    }

    fn adjust_adaptive_batch_size(&mut self) {
        // Simple adaptive algorithm: increase batch size if we're consistently flushing due to size
        // decrease if we're often flushing due to timeout
        let size_ratio = self.stats.total_batches as f64 / (self.stats.flush_timeouts + 1) as f64;
        
        if size_ratio > 2.0 && self.adaptive_batch_size < self.config.max_batch_size {
            // Mostly size-based flushes, can increase batch size
            self.adaptive_batch_size = (self.adaptive_batch_size * 11 / 10).min(self.config.max_batch_size);
            self.stats.adaptive_adjustments += 1;
        } else if size_ratio < 0.5 && self.adaptive_batch_size > 10 {
            // Mostly timeout-based flushes, should decrease batch size
            self.adaptive_batch_size = (self.adaptive_batch_size * 9 / 10).max(10);
            self.stats.adaptive_adjustments += 1;
        }
    }
}

/// Async command batcher for concurrent scenarios
pub struct AsyncCommandBatcher {
    command_tx: mpsc::UnboundedSender<Command>,
    batch_rx: mpsc::UnboundedReceiver<CommandBatch>,
    stats_tx: mpsc::UnboundedSender<BatchStats>,
    _task_handle: tokio::task::JoinHandle<()>,
}

impl AsyncCommandBatcher {
    pub fn new(config: BatchConfig) -> Self {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let (batch_tx, batch_rx) = mpsc::unbounded_channel();
        let (stats_tx, mut stats_rx) = mpsc::unbounded_channel();
        
        let config_clone = config.clone();
        let task_handle = tokio::spawn(async move {
            let mut batcher = CommandBatcher::new(config_clone);
            let mut flush_interval = tokio::time::interval(config.max_batch_delay);
            
            loop {
                tokio::select! {
                    // Process incoming commands
                    command = command_rx.recv() => {
                        match command {
                            Some(cmd) => {
                                match batcher.add_command(cmd) {
                                    Ok(Some(batch)) => {
                                        if batch_tx.send(batch).is_err() {
                                            break; // Receiver dropped
                                        }
                                    }
                                    Ok(None) => {} // Command buffered
                                    Err(_) => {} // Buffer overflow, command dropped
                                }
                            }
                            None => break, // Command sender dropped
                        }
                    }
                    
                    // Periodic flush for time-based batching
                    _ = flush_interval.tick() => {
                        if let Some(batch) = batcher.flush() {
                            if batch_tx.send(batch).is_err() {
                                break; // Receiver dropped
                            }
                        }
                    }
                    
                    // Handle stats requests
                    _ = stats_rx.recv() => {
                        // Stats request received, sender already has reference to send back
                    }
                }
            }
        });

        Self {
            command_tx,
            batch_rx,
            stats_tx,
            _task_handle: task_handle,
        }
    }

    /// Add a command for batching
    pub fn add_command(&self, command: Command) -> Result<()> {
        self.command_tx.send(command)
            .map_err(|_| RabiaError::internal("Batcher task has stopped"))
    }

    /// Receive next batch (blocking)
    pub async fn next_batch(&mut self) -> Option<CommandBatch> {
        self.batch_rx.recv().await
    }

    /// Try to receive a batch without blocking
    pub fn try_next_batch(&mut self) -> Option<CommandBatch> {
        self.batch_rx.try_recv().ok()
    }

    /// Receive next batch with timeout
    pub async fn next_batch_timeout(&mut self, duration: Duration) -> Result<CommandBatch> {
        timeout(duration, self.batch_rx.recv())
            .await
            .map_err(|_| RabiaError::Timeout { operation: "batch receive".to_string() })?
            .ok_or_else(|| RabiaError::internal("Batcher task has stopped"))
    }
}

/// Batch processor that applies multiple commands efficiently
pub struct BatchProcessor {
    /// Optional command transformation function
    transform_fn: Option<fn(&mut Command)>,
    /// Enable parallel processing for independent commands
    parallel: bool,
}

impl BatchProcessor {
    pub fn new() -> Self {
        Self {
            transform_fn: None,
            parallel: false,
        }
    }

    pub fn with_transform(mut self, transform_fn: fn(&mut Command)) -> Self {
        self.transform_fn = Some(transform_fn);
        self
    }

    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Process a batch of commands efficiently
    pub async fn process_batch<F, Fut>(&self, batch: CommandBatch, mut processor: F) -> Result<Vec<bytes::Bytes>>
    where
        F: FnMut(&Command) -> Fut,
        Fut: std::future::Future<Output = Result<bytes::Bytes>>,
    {
        let mut commands = batch.commands;
        
        // Apply transformation if configured
        if let Some(transform) = self.transform_fn {
            for command in &mut commands {
                transform(command);
            }
        }

        if self.parallel && commands.len() > 1 {
            // Process commands in parallel
            let futures = commands.iter().map(|cmd| processor(cmd));
            let results = futures_util::future::try_join_all(futures).await?;
            Ok(results)
        } else {
            // Process commands sequentially
            let mut results = Vec::with_capacity(commands.len());
            for command in &commands {
                results.push(processor(command).await?);
            }
            Ok(results)
        }
    }
}

impl Default for BatchProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_batcher_basic() {
        let config = BatchConfig {
            max_batch_size: 3,
            max_batch_delay: Duration::from_millis(100),
            buffer_capacity: 10,
            adaptive: false,
        };
        
        let mut batcher = CommandBatcher::new(config);
        
        // Add commands one by one
        assert!(batcher.add_command(Command::new("SET key1 value1")).unwrap().is_none());
        assert!(batcher.add_command(Command::new("SET key2 value2")).unwrap().is_none());
        
        // Third command should trigger flush
        let batch = batcher.add_command(Command::new("SET key3 value3")).unwrap();
        assert!(batch.is_some());
        
        let batch = batch.unwrap();
        assert_eq!(batch.commands.len(), 3);
    }

    #[test]
    fn test_batcher_flush() {
        let config = BatchConfig::default();
        let mut batcher = CommandBatcher::new(config);
        
        batcher.add_command(Command::new("SET key1 value1")).unwrap();
        batcher.add_command(Command::new("SET key2 value2")).unwrap();
        
        let batch = batcher.flush().unwrap();
        assert_eq!(batch.commands.len(), 2);
        assert!(batcher.is_empty());
    }

    #[tokio::test]
    async fn test_async_command_batcher() {
        let config = BatchConfig {
            max_batch_size: 2,
            max_batch_delay: Duration::from_millis(50),
            buffer_capacity: 10,
            adaptive: false,
        };
        
        let mut async_batcher = AsyncCommandBatcher::new(config);
        
        // Add commands
        async_batcher.add_command(Command::new("SET key1 value1")).unwrap();
        async_batcher.add_command(Command::new("SET key2 value2")).unwrap();
        
        // Should get a batch immediately due to size limit
        let batch = async_batcher.next_batch().await.unwrap();
        assert_eq!(batch.commands.len(), 2);
    }

    #[tokio::test]
    async fn test_async_batcher_timeout() {
        let config = BatchConfig {
            max_batch_size: 10,
            max_batch_delay: Duration::from_millis(50),
            buffer_capacity: 10,
            adaptive: false,
        };
        
        let mut async_batcher = AsyncCommandBatcher::new(config);
        
        // Add a single command
        async_batcher.add_command(Command::new("SET key1 value1")).unwrap();
        
        // Should get a batch after timeout
        let batch = async_batcher.next_batch_timeout(Duration::from_millis(100)).await.unwrap();
        assert_eq!(batch.commands.len(), 1);
    }

    #[tokio::test]
    async fn test_batch_processor() {
        let processor = BatchProcessor::new().with_parallel(false);
        
        let commands = vec![
            Command::new("SET key1 value1"),
            Command::new("SET key2 value2"),
        ];
        let batch = CommandBatch::new(commands);
        
        let results = processor.process_batch(batch, |cmd| {
            let data = cmd.data.clone();
            async move {
                Ok(bytes::Bytes::from(format!("processed: {}", String::from_utf8_lossy(&data))))
            }
        }).await.unwrap();
        
        assert_eq!(results.len(), 2);
        assert!(results[0].starts_with(b"processed:"));
    }
}