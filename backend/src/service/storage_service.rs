use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{Config, StorageBackend};
use crate::storage::{IpfsStorageAdapter, LocalStorageAdapter, S3StorageAdapter, StorageAdapter};

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
                Arc::new(LocalStorageAdapter::new(
                    PathBuf::from(base_path),
                    public_base_url,
                ))
            }
            StorageBackend::S3 => Arc::new(S3StorageAdapter::new()),
            StorageBackend::Ipfs => Arc::new(IpfsStorageAdapter::new()),
        };

        Self { adapter }
    }
}
