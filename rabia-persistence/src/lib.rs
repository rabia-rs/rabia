//! # Rabia Persistence
//!
//! Simple persistence implementations for the Rabia consensus protocol.
//!
//! This crate provides simplified persistence implementations that store
//! exactly one state value, matching Rabia's consensus requirements.
//!
//! ## Implementations
//!
//! - [`InMemoryPersistence`] - State stored in memory (testing/non-persistent)
//! - [`FileSystemPersistence`] - State stored in a file (persistent across restarts)
//!
//! ## Example
//!
//! ```rust
//! use rabia_persistence::{InMemoryPersistence, FileSystemPersistence};
//! use rabia_core::persistence::PersistenceLayer;
//!
//! # tokio_test::block_on(async {
//! // In-memory persistence
//! let persistence = InMemoryPersistence::new();
//! persistence.save_state(b"hello world").await.unwrap();
//! let state = persistence.load_state().await.unwrap();
//! assert_eq!(state, Some(b"hello world".to_vec()));
//!
//! // File-based persistence  
//! let fs_persistence = FileSystemPersistence::new("/tmp/rabia-test").await.unwrap();
//! fs_persistence.save_state(b"persistent state").await.unwrap();
//! let state = fs_persistence.load_state().await.unwrap();
//! assert_eq!(state, Some(b"persistent state".to_vec()));
//! # });
//! ```

pub mod file_system;
pub mod in_memory;
mod tests;

pub use file_system::FileSystemPersistence;
pub use in_memory::InMemoryPersistence;
