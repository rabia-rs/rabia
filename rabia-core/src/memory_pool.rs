use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use bytes::Bytes;

/// A simple memory pool for reusing byte buffers to reduce allocations
pub struct MemoryPool {
    small_buffers: Arc<Mutex<VecDeque<Vec<u8>>>>,
    medium_buffers: Arc<Mutex<VecDeque<Vec<u8>>>>,
    large_buffers: Arc<Mutex<VecDeque<Vec<u8>>>>,
    max_pool_size: usize,
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub small_buffer_size: usize,
    pub medium_buffer_size: usize,
    pub large_buffer_size: usize,
    pub max_pool_size: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            small_buffer_size: 1024,      // 1KB for typical messages
            medium_buffer_size: 8192,     // 8KB for batch messages
            large_buffer_size: 65536,     // 64KB for large batches
            max_pool_size: 100,           // Max pooled buffers per size
        }
    }
}

impl MemoryPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            small_buffers: Arc::new(Mutex::new(VecDeque::with_capacity(config.max_pool_size))),
            medium_buffers: Arc::new(Mutex::new(VecDeque::with_capacity(config.max_pool_size))),
            large_buffers: Arc::new(Mutex::new(VecDeque::with_capacity(config.max_pool_size))),
            max_pool_size: config.max_pool_size,
        }
    }

    /// Get a buffer of appropriate size from the pool
    pub fn get_buffer(&self, min_size: usize) -> PooledBuffer {
        let (buffer, pool, original_capacity) = if min_size <= 1024 {
            let mut pool = self.small_buffers.lock().unwrap();
            let buffer = pool.pop_front().unwrap_or_else(|| Vec::with_capacity(1024));
            (buffer, self.small_buffers.clone(), 1024)
        } else if min_size <= 8192 {
            let mut pool = self.medium_buffers.lock().unwrap();
            let buffer = pool.pop_front().unwrap_or_else(|| Vec::with_capacity(8192));
            (buffer, self.medium_buffers.clone(), 8192)
        } else {
            let mut pool = self.large_buffers.lock().unwrap();
            let buffer = pool.pop_front().unwrap_or_else(|| Vec::with_capacity(65536.max(min_size)));
            (buffer, self.large_buffers.clone(), 65536.max(min_size))
        };

        PooledBuffer {
            buffer,
            pool,
            original_capacity,
            max_pool_size: self.max_pool_size,
        }
    }

    /// Pre-warm the pool with buffers
    pub fn warm_up(&self) {
        let config = PoolConfig::default();
        
        // Pre-allocate some buffers
        for _ in 0..10 {
            self.small_buffers.lock().unwrap()
                .push_back(Vec::with_capacity(config.small_buffer_size));
            self.medium_buffers.lock().unwrap()
                .push_back(Vec::with_capacity(config.medium_buffer_size));
            self.large_buffers.lock().unwrap()
                .push_back(Vec::with_capacity(config.large_buffer_size));
        }
    }

    /// Get statistics about pool usage
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            small_buffers_available: self.small_buffers.lock().unwrap().len(),
            medium_buffers_available: self.medium_buffers.lock().unwrap().len(),
            large_buffers_available: self.large_buffers.lock().unwrap().len(),
        }
    }

    /// Clear all pooled buffers
    pub fn clear(&self) {
        self.small_buffers.lock().unwrap().clear();
        self.medium_buffers.lock().unwrap().clear();
        self.large_buffers.lock().unwrap().clear();
    }
}

impl Default for MemoryPool {
    fn default() -> Self {
        Self::new(PoolConfig::default())
    }
}

/// A buffer that returns to the pool when dropped
pub struct PooledBuffer {
    buffer: Vec<u8>,
    pool: Arc<Mutex<VecDeque<Vec<u8>>>>,
    original_capacity: usize,
    max_pool_size: usize,
}

impl PooledBuffer {
    /// Get the underlying buffer
    pub fn buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    /// Take ownership of the buffer contents as Bytes
    pub fn take_bytes(&mut self) -> Bytes {
        Bytes::from(std::mem::take(&mut self.buffer))
    }

    /// Get a reference to the buffer
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Get the length of the buffer
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Resize the buffer
    pub fn resize(&mut self, new_len: usize, value: u8) {
        self.buffer.resize(new_len, value);
    }

    /// Clear the buffer content but keep capacity
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Clear the buffer but keep capacity
        self.buffer.clear();
        
        // Only return to pool if it's the right size and pool isn't full
        if self.buffer.capacity() == self.original_capacity {
            let mut pool = self.pool.lock().unwrap();
            if pool.len() < self.max_pool_size {
                pool.push_back(std::mem::take(&mut self.buffer));
            }
        }
    }
}

#[derive(Debug)]
pub struct PoolStats {
    pub small_buffers_available: usize,
    pub medium_buffers_available: usize,
    pub large_buffers_available: usize,
}

/// Thread-local memory pool for high-performance scenarios
thread_local! {
    static LOCAL_POOL: MemoryPool = {
        let pool = MemoryPool::default();
        pool.warm_up();
        pool
    };
}

/// Get a buffer from the thread-local pool
pub fn get_pooled_buffer(min_size: usize) -> PooledBuffer {
    LOCAL_POOL.with(|pool| pool.get_buffer(min_size))
}

/// Optimized string pool for command strings
pub struct StringPool {
    small_strings: Arc<Mutex<VecDeque<String>>>,
    large_strings: Arc<Mutex<VecDeque<String>>>,
    max_pool_size: usize,
}

impl StringPool {
    pub fn new(max_pool_size: usize) -> Self {
        Self {
            small_strings: Arc::new(Mutex::new(VecDeque::with_capacity(max_pool_size))),
            large_strings: Arc::new(Mutex::new(VecDeque::with_capacity(max_pool_size))),
            max_pool_size,
        }
    }

    pub fn get_string(&self, min_capacity: usize) -> PooledString {
        let (string, pool, is_large) = if min_capacity <= 256 {
            let mut pool = self.small_strings.lock().unwrap();
            let string = pool.pop_front().unwrap_or_else(|| String::with_capacity(256));
            (string, self.small_strings.clone(), false)
        } else {
            let mut pool = self.large_strings.lock().unwrap();
            let string = pool.pop_front().unwrap_or_else(|| String::with_capacity(1024.max(min_capacity)));
            (string, self.large_strings.clone(), true)
        };

        PooledString {
            string,
            pool,
            is_large,
            max_pool_size: self.max_pool_size,
        }
    }
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new(50)
    }
}

pub struct PooledString {
    string: String,
    pool: Arc<Mutex<VecDeque<String>>>,
    is_large: bool,
    max_pool_size: usize,
}

impl PooledString {
    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn push_str(&mut self, s: &str) {
        self.string.push_str(s);
    }

    pub fn clear(&mut self) {
        self.string.clear();
    }

    pub fn into_string(mut self) -> String {
        // Prevent drop from returning to pool
        let mut string = String::new();
        std::mem::swap(&mut string, &mut self.string);
        std::mem::forget(self);
        string
    }
}

impl Drop for PooledString {
    fn drop(&mut self) {
        self.string.clear();
        let mut pool = self.pool.lock().unwrap();
        if pool.len() < self.max_pool_size {
            pool.push_back(std::mem::take(&mut self.string));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_basic() {
        let pool = MemoryPool::default();
        
        let mut buffer = pool.get_buffer(100);
        buffer.buffer_mut().extend_from_slice(b"hello world");
        assert_eq!(buffer.len(), 11);
        
        // Drop the buffer, should return to pool
        drop(buffer);
        
        // Get another buffer, should reuse the previous one
        let stats = pool.stats();
        assert!(stats.small_buffers_available > 0);
    }

    #[test]
    fn test_different_buffer_sizes() {
        let pool = MemoryPool::default();
        
        let small = pool.get_buffer(500);      // Should use small pool
        let medium = pool.get_buffer(4000);    // Should use medium pool  
        let large = pool.get_buffer(32000);    // Should use large pool
        
        assert!(small.buffer.capacity() >= 500);
        assert!(medium.buffer.capacity() >= 4000);
        assert!(large.buffer.capacity() >= 32000);
    }

    #[test]
    fn test_thread_local_pool() {
        let buffer1 = get_pooled_buffer(100);
        let buffer2 = get_pooled_buffer(200);
        
        assert!(buffer1.buffer.capacity() >= 100);
        assert!(buffer2.buffer.capacity() >= 200);
    }

    #[test]
    fn test_string_pool() {
        let pool = StringPool::default();
        
        let mut string = pool.get_string(100);
        string.push_str("hello");
        string.push_str(" world");
        
        assert_eq!(string.as_str(), "hello world");
        
        // Test conversion to owned string
        let owned = string.into_string();
        assert_eq!(owned, "hello world");
    }

    #[test]
    fn test_pooled_buffer_bytes() {
        let pool = MemoryPool::default();
        let mut buffer = pool.get_buffer(100);
        
        buffer.buffer_mut().extend_from_slice(b"test data");
        let bytes = buffer.take_bytes();
        
        assert_eq!(bytes.as_ref(), b"test data");
        assert_eq!(buffer.len(), 0); // Buffer should be empty after taking bytes
    }
}