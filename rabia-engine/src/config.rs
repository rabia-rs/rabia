use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RabiaConfig {
    pub phase_timeout: Duration,
    pub sync_timeout: Duration,
    pub max_batch_size: usize,
    pub max_pending_batches: usize,
    pub cleanup_interval: Duration,
    pub max_phase_history: usize,
    pub heartbeat_interval: Duration,
    pub randomization_seed: Option<u64>,
    pub max_retries: usize,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
}

impl Default for RabiaConfig {
    fn default() -> Self {
        Self {
            phase_timeout: Duration::from_millis(5000),
            sync_timeout: Duration::from_millis(10000),
            max_batch_size: 1000,
            max_pending_batches: 100,
            cleanup_interval: Duration::from_secs(30),
            max_phase_history: 1000,
            heartbeat_interval: Duration::from_millis(1000),
            randomization_seed: None,
            max_retries: 3,
            backoff_base: Duration::from_millis(100),
            backoff_max: Duration::from_secs(10),
        }
    }
}

impl RabiaConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_phase_timeout(mut self, timeout: Duration) -> Self {
        self.phase_timeout = timeout;
        self
    }

    pub fn with_sync_timeout(mut self, timeout: Duration) -> Self {
        self.sync_timeout = timeout;
        self
    }

    pub fn with_max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    pub fn with_cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }

    pub fn with_randomization_seed(mut self, seed: u64) -> Self {
        self.randomization_seed = Some(seed);
        self
    }
}
