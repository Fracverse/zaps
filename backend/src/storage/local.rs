use std::{fs, path::PathBuf};

use async_trait::async_trait;
use bytes::Bytes;
use uuid::Uuid;

use super::{StorageAdapter, StoredFile};

#[derive(Clone)]
pub struct LocalStorageAdapter {
    base_path: PathBuf,
    public_base_url: String,
}

impl LocalStorageAdapter {
    pub fn new(base_path: PathBuf, public_base_url: String) -> Self {
        fs::create_dir_all(&base_path).ok();
        Self {
            base_path,
            public_base_url,
        }
    }
}

#[async_trait]
impl StorageAdapter for LocalStorageAdapter {
    async fn upload(&self, data: Bytes, file_name: &str, mime_type: &str) -> anyhow::Result<StoredFile> {
        let id = Uuid::new_v4().to_string();
        let mut path = self.base_path.clone();
        path.push(&id);

        fs::write(&path, &data)?;

        Ok(StoredFile {
            id: id.clone(),
            original_name: file_name.to_string(),
            mime_type: mime_type.to_string(),
            size: data.len() as i64,
            url: format!("{}/{}", self.public_base_url.trim_end_matches('/'), id),
        })
    }

    async fn get(&self, id: &str) -> anyhow::Result<Option<StoredFile>> {
        let mut path = self.base_path.clone();
        path.push(id);

        if !path.exists() {
            return Ok(None);
        }

        let metadata = fs::metadata(&path)?;
        Ok(Some(StoredFile {
            id: id.to_string(),
            original_name: id.to_string(),
            mime_type: "application/octet-stream".to_string(),
            size: metadata.len() as i64,
            url: format!("{}/{}", self.public_base_url.trim_end_matches('/'), id),
        }))
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let mut path = self.base_path.clone();
        path.push(id);

        if path.exists() {
            let _ = fs::remove_file(path);
        }

        Ok(())
    }
}
