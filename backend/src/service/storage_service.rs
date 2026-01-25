use std::path::PathBuf;

use crate::config::{Config, StorageBackend};
use crate::storage::{IPFSStorageAdapter, LocalStorageAdapter, S3StorageAdapter, StorageAdapter};
use std::sync::Arc;

#[derive(Clone)]
pub struct StorageService {
    pub adapter: Arc<dyn StorageAdapter>,
}

impl StorageService {
    pub fn new(config: Config) -> Self {
        let adapter: Arc<dyn StorageAdapter> = match config.storage.backend {
            StorageBackend::Local => {
                let base_path = config
                    .storage
                    .local_path
                    .clone()
                    .unwrap_or_else(|| "./uploads".to_string());
                let public_base_url = "/files".to_string();
                Arc::new(LocalStorageAdapter::new(PathBuf::from(base_path), public_base_url))
            }
            StorageBackend::S3 => Arc::new(S3StorageAdapter::new()),
            StorageBackend::Ipfs => Arc::new(IPFSStorageAdapter::new()),
        };

        Self { adapter }
    }
}
