use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{Multipart, Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};

use crate::{
    api_error::ApiError,
    models::FileUploadResponseDto,
    service::ServiceContainer,
};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const ALLOWED_MIME_TYPES: &[&str] = &["image/png", "image/jpeg", "application/pdf"];

async fn virus_scan_placeholder(_data: &Bytes) -> Result<(), ApiError> {
    // Virus scan hook placeholder (always passes).
    // TODO: Integrate with a real AV engine (e.g. ClamAV) when available.
    Ok(())
}

pub async fn upload_file(
    State(services): State<Arc<ServiceContainer>>,
    mut multipart: Multipart,
) -> Result<Json<FileUploadResponseDto>, ApiError> {
    let mut file_name = None;
    let mut content_type = None;
    let mut data = Bytes::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::Validation(format!("Failed to parse multipart form: {}", e)))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());

            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::Validation(format!("Failed to read file bytes: {}", e)))?;

            if bytes.len() as u64 > MAX_FILE_SIZE {
                return Err(ApiError::Validation(
                    "File size exceeds 10MB limit".to_string(),
                ));
            }

            data = bytes;
            break;
        }
    }

    let file_name = file_name
        .ok_or_else(|| ApiError::Validation("file field is required".to_string()))?;
    let mime_type = content_type
        .ok_or_else(|| ApiError::Validation("Missing content type".to_string()))?;

    if !ALLOWED_MIME_TYPES.contains(&mime_type.as_str()) {
        return Err(ApiError::Validation("Unsupported file type".to_string()));
    }

    virus_scan_placeholder(&data).await?;

    let stored = services
        .storage
        .adapter
        .upload(data, &file_name, &mime_type)
        .map_err(|_| ApiError::InternalServerError)?;

    Ok(Json(FileUploadResponseDto {
        file_id: stored.id,
        original_name: stored.original_name,
        mime_type: stored.mime_type,
        size: stored.size,
        url: stored.url,
    }))
}

pub async fn get_file(
    State(services): State<Arc<ServiceContainer>>,
    Path(id): Path<String>,
) -> Result<(HeaderMap, Body), ApiError> {
    // Serve raw bytes (local storage only). For non-local backends this may fail until implemented.
    let stored = services
        .storage
        .adapter
        .get(&id)
        .map_err(|_| ApiError::InternalServerError)?;

    let stored = stored.ok_or_else(|| ApiError::NotFound("File not found".to_string()))?;

    // LocalStorageAdapter is currently storing files as-is under `storage.local_path` with filename == id.
    // We read the file directly based on config.
    let base_path = services
        .config
        .storage
        .local_path
        .clone()
        .unwrap_or_else(|| "./uploads".to_string());

    let file_path = std::path::Path::new(&base_path).join(&stored.id);
    let bytes = tokio::fs::read(file_path)
        .await
        .map_err(|_| ApiError::NotFound("File not found".to_string()))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        stored
            .mime_type
            .parse()
            .unwrap_or_else(|_| header::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_str(&format!(
            "inline; filename=\"{}\"",
            stored.original_name
        ))
        .unwrap_or_else(|_| header::HeaderValue::from_static("inline")),
    );

    Ok((headers, Body::from(bytes)))
}

// New: JSON metadata endpoint (keeps previous behavior)
pub async fn get_file_metadata(
    State(services): State<Arc<ServiceContainer>>,
    Path(id): Path<String>,
) -> Result<Json<FileUploadResponseDto>, ApiError> {
    let stored = services
        .storage
        .adapter
        .get(&id)
        .map_err(|_| ApiError::InternalServerError)?;

    let stored = stored.ok_or_else(|| ApiError::NotFound("File not found".to_string()))?;

    Ok(Json(FileUploadResponseDto {
        file_id: stored.id,
        original_name: stored.original_name,
        mime_type: stored.mime_type,
        size: stored.size,
        url: stored.url,
    }))
}

pub async fn delete_file(
    State(services): State<Arc<ServiceContainer>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    services
        .storage
        .adapter
        .delete(&id)
        .map_err(|_| ApiError::InternalServerError)?;

    Ok(StatusCode::NO_CONTENT)
}
