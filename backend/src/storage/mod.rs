use async_trait::async_trait;
use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct StoredFile {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub url: String,
}

#[async_trait]
pub trait StorageAdapter: Send + Sync {
    async fn upload(&self, data: Bytes, file_name: &str, mime_type: &str) -> anyhow::Result<StoredFile>;
    async fn get(&self, id: &str) -> anyhow::Result<Option<StoredFile>>;
    async fn delete(&self, id: &str) -> anyhow::Result<()>;
}

pub mod local;
pub mod s3;
pub mod ipfs;

pub use local::LocalStorageAdapter;
pub use s3::S3StorageAdapter;
pub use ipfs::IPFSStorageAdapter;
