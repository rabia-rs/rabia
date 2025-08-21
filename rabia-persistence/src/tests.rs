#[cfg(test)]
mod unit_tests {
    use crate::{FileSystemPersistence, InMemoryPersistence};
    use rabia_core::persistence::PersistenceLayer;

    #[tokio::test]
    async fn test_in_memory_persistence() {
        let persistence = InMemoryPersistence::new();

        // Initially should return None
        assert!(persistence.load_state().await.unwrap().is_none());

        // Save some data
        let test_data = b"hello world";
        persistence.save_state(test_data).await.unwrap();

        // Load should return the saved data
        let loaded = persistence.load_state().await.unwrap();
        assert_eq!(loaded, Some(test_data.to_vec()));

        // Overwrite with new data
        let new_data = b"goodbye world";
        persistence.save_state(new_data).await.unwrap();

        let loaded = persistence.load_state().await.unwrap();
        assert_eq!(loaded, Some(new_data.to_vec()));
    }

    #[tokio::test]
    async fn test_file_system_persistence() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let persistence = FileSystemPersistence::new(temp_dir.path()).await.unwrap();

        // Initially should return None
        assert!(persistence.load_state().await.unwrap().is_none());

        // Save some data
        let test_data = b"persistent data";
        persistence.save_state(test_data).await.unwrap();

        // Load should return the saved data
        let loaded = persistence.load_state().await.unwrap();
        assert_eq!(loaded, Some(test_data.to_vec()));

        // Create a new instance with the same directory
        let persistence2 = FileSystemPersistence::new(temp_dir.path()).await.unwrap();

        // Should still load the same data
        let loaded2 = persistence2.load_state().await.unwrap();
        assert_eq!(loaded2, Some(test_data.to_vec()));

        // Overwrite with new data
        let new_data = b"updated persistent data";
        persistence2.save_state(new_data).await.unwrap();

        let loaded3 = persistence2.load_state().await.unwrap();
        assert_eq!(loaded3, Some(new_data.to_vec()));
    }

    #[tokio::test]
    async fn test_empty_data() {
        let persistence = InMemoryPersistence::new();

        // Save empty data
        let empty_data = b"";
        persistence.save_state(empty_data).await.unwrap();

        // Should be able to load empty data
        let loaded = persistence.load_state().await.unwrap();
        assert_eq!(loaded, Some(empty_data.to_vec()));
    }

    #[tokio::test]
    async fn test_large_data() {
        let persistence = InMemoryPersistence::new();

        // Create 1MB of test data
        let large_data = vec![0xAB; 1024 * 1024];
        persistence.save_state(&large_data).await.unwrap();

        let loaded = persistence.load_state().await.unwrap();
        assert_eq!(loaded, Some(large_data));
    }
}
