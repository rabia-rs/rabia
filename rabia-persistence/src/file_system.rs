use async_trait::async_trait;
use rabia_core::{persistence::PersistenceLayer, RabiaError, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Simple file-based persistence implementation.
///
/// This implementation stores the state in a single file on disk. It provides
/// persistent storage that survives process restarts.
#[derive(Debug, Clone)]
pub struct FileSystemPersistence {
    state_file_path: PathBuf,
}

impl FileSystemPersistence {
    /// Create a new file-based persistence instance.
    ///
    /// # Arguments
    /// * `data_dir` - Directory path where the state file will be stored
    ///
    /// # Returns
    /// * A new `FileSystemPersistence` instance
    ///
    /// # Errors
    /// * Returns error if the data directory cannot be created
    pub async fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        let data_dir = data_dir.as_ref();

        // Create data directory if it doesn't exist
        if !data_dir.exists() {
            fs::create_dir_all(data_dir).await.map_err(|e| {
                RabiaError::persistence(format!("Failed to create data directory: {}", e))
            })?;
        }

        let state_file_path = data_dir.join("state.dat");

        Ok(Self { state_file_path })
    }

    /// Create a new file-based persistence instance (synchronous).
    ///
    /// This is a convenience method that blocks on the async `new` method.
    ///
    /// # Arguments
    /// * `data_dir` - Directory path where the state file will be stored
    ///
    /// # Returns
    /// * A new `FileSystemPersistence` instance
    ///
    /// # Errors
    /// * Returns error if the data directory cannot be created
    pub fn new_sync<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| RabiaError::internal(format!("Failed to create runtime: {}", e)))?;
        runtime.block_on(Self::new(data_dir))
    }
}

#[async_trait]
impl PersistenceLayer for FileSystemPersistence {
    async fn save_state(&self, state: &[u8]) -> Result<()> {
        // Write to a temporary file first, then atomically move to final location
        let temp_file_path = self.state_file_path.with_extension("tmp");

        fs::write(&temp_file_path, state).await.map_err(|e| {
            RabiaError::persistence(format!("Failed to write state to temp file: {}", e))
        })?;

        // Atomically replace the old file with the new one
        fs::rename(&temp_file_path, &self.state_file_path)
            .await
            .map_err(|e| {
                RabiaError::persistence(format!("Failed to rename temp file to state file: {}", e))
            })?;

        Ok(())
    }

    async fn load_state(&self) -> Result<Option<Vec<u8>>> {
        if !self.state_file_path.exists() {
            return Ok(None);
        }

        match fs::read(&self.state_file_path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(RabiaError::persistence(format!(
                "Failed to read state file: {}",
                e
            ))),
        }
    }
}
