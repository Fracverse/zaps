use axum::body::Bytes;
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StoredFile {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: u64,
    pub url: String,
}

pub trait StorageAdapter: Send + Sync {
    fn upload(
        &self,
        data: Bytes,
        original_name: &str,
        mime_type: &str,
    ) -> Result<StoredFile, Box<dyn std::error::Error + Send + Sync>>;

    fn get(&self, id: &str) -> Result<Option<StoredFile>, Box<dyn std::error::Error + Send + Sync>>;

    fn delete(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[derive(Debug, Clone)]
pub struct LocalStorageAdapter {
    base_path: PathBuf,
    public_base_url: String,
}

impl LocalStorageAdapter {
    pub fn new(base_path: PathBuf, public_base_url: String) -> Self {
        Self {
            base_path,
            public_base_url,
        }
    }

    fn ensure_base_dir(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(&self.base_path)?;
        Ok(())
    }

    fn path_for_id(&self, id: &str) -> PathBuf {
        self.base_path.join(id)
    }

    fn url_for_id(&self, id: &str) -> String {
        format!("{}/{}", self.public_base_url.trim_end_matches('/'), id)
    }
}

impl StorageAdapter for LocalStorageAdapter {
    fn upload(
        &self,
        data: Bytes,
        original_name: &str,
        mime_type: &str,
    ) -> Result<StoredFile, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_base_dir()?;

        let id = Uuid::new_v4().to_string();
        let path = self.path_for_id(&id);
        fs::write(&path, &data)?;

        Ok(StoredFile {
            id,
            original_name: original_name.to_string(),
            mime_type: mime_type.to_string(),
            size: data.len() as u64,
            url: self.url_for_id(path.file_name().unwrap_or_default().to_string_lossy().as_ref()),
        })
    }

    fn get(&self, id: &str) -> Result<Option<StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path_for_id(id);
        if !Path::new(&path).exists() {
            return Ok(None);
        }

        let meta = fs::metadata(&path)?;
        Ok(Some(StoredFile {
            id: id.to_string(),
            original_name: id.to_string(),
            mime_type: "application/octet-stream".to_string(),
            size: meta.len(),
            url: self.url_for_id(id),
        }))
    }

    fn delete(&self, id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path_for_id(id);
        if Path::new(&path).exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct S3StorageAdapter;
impl S3StorageAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl StorageAdapter for S3StorageAdapter {
    fn upload(
        &self,
        _data: Bytes,
        _original_name: &str,
        _mime_type: &str,
    ) -> Result<StoredFile, Box<dyn std::error::Error + Send + Sync>> {
        Err("S3 adapter not implemented".into())
    }

    fn get(&self, _id: &str) -> Result<Option<StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        Err("S3 adapter not implemented".into())
    }

    fn delete(&self, _id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("S3 adapter not implemented".into())
    }
}

#[derive(Debug, Clone)]
pub struct IpfsStorageAdapter;
impl IpfsStorageAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl StorageAdapter for IpfsStorageAdapter {
    fn upload(
        &self,
        _data: Bytes,
        _original_name: &str,
        _mime_type: &str,
    ) -> Result<StoredFile, Box<dyn std::error::Error + Send + Sync>> {
        Err("IPFS adapter not implemented".into())
    }

    fn get(&self, _id: &str) -> Result<Option<StoredFile>, Box<dyn std::error::Error + Send + Sync>> {
        Err("IPFS adapter not implemented".into())
    }

    fn delete(&self, _id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("IPFS adapter not implemented".into())
    }
}
