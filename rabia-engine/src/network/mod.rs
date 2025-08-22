//! Network transport implementations for the Rabia engine
//!
//! This module provides networking capabilities as a core component of the Rabia engine.
//! TCP networking is the default production implementation.

pub mod tcp;

pub use tcp::{BufferConfig, RetryConfig, TcpNetwork, TcpNetworkConfig};
