use async_trait::async_trait;
use bytes::Bytes;

use super::{StorageAdapter, StoredFile};

#[derive(Clone)]
pub struct S3StorageAdapter;

impl S3StorageAdapter {
    pub fn new() -> Self {
        // TODO: wire real S3 client and bucket config
        Self
    }
}

#[async_trait]
impl StorageAdapter for S3StorageAdapter {
    async fn upload(&self, _data: Bytes, _file_name: &str, _mime_type: &str) -> anyhow::Result<StoredFile> {
        // Placeholder implementation
        Err(anyhow::anyhow!("S3StorageAdapter is not implemented"))
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
