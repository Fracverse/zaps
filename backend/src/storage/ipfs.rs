use async_trait::async_trait;
use bytes::Bytes;

use super::{StorageAdapter, StoredFile};

#[derive(Clone)]
pub struct IPFSStorageAdapter;

impl IPFSStorageAdapter {
    pub fn new() -> Self {
        // TODO: wire real IPFS client and gateway config
        Self
    }
}

#[async_trait]
impl StorageAdapter for IPFSStorageAdapter {
    async fn upload(&self, _data: Bytes, _file_name: &str, _mime_type: &str) -> anyhow::Result<StoredFile> {
        // Placeholder implementation
        Err(anyhow::anyhow!("IPFSStorageAdapter is not implemented"))
    }

    async fn get(&self, _id: &str) -> anyhow::Result<Option<StoredFile>> {
        // Placeholder implementation
        Ok(None)
    }

    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        // Placeholder implementation
        Ok(())
    }
}
