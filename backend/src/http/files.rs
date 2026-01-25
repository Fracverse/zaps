use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    api_error::ApiError,
    models::FileUploadResponseDto,
    service::ServiceContainer,
};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const ALLOWED_MIME_TYPES: &[&str] = &["image/png", "image/jpeg", "application/pdf"];

pub async fn upload_file(
    State(services): State<Arc<ServiceContainer>>,
    mut multipart: Multipart,
) -> Result<Json<FileUploadResponseDto>, ApiError> {
    let mut file_name = None;
    let mut content_type = None;
    let mut data = Bytes::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError::Validation(format!("Failed to parse multipart form: {}", e))
    })? {
        if field.name() == Some("file") {
            file_name = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());

            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::Validation(format!("Failed to read file bytes: {}", e)))?;

            if bytes.len() as u64 > MAX_FILE_SIZE {
                return Err(ApiError::Validation("File size exceeds 10MB limit".to_string()));
            }

            data = bytes;
            break;
        }
    }

    let file_name = file_name.ok_or_else(|| ApiError::Validation("file field is required".to_string()))?;
    let mime_type = content_type.ok_or_else(|| ApiError::Validation("Missing content type".to_string()))?;

    if !ALLOWED_MIME_TYPES.contains(&mime_type.as_str()) {
        return Err(ApiError::Validation("Unsupported file type".to_string()));
    }

    // Placeholder virus scan hook - always passes. Replace with real AV integration.
    // In a real implementation, you would stream `data` to an antivirus engine here.

    let stored = services
        .storage
        .adapter
        .upload(data, &file_name, &mime_type)
        .await
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
) -> Result<Json<FileUploadResponseDto>, ApiError> {
    let stored = services
        .storage
        .adapter
        .get(&id)
        .await
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
        .await
        .map_err(|_| ApiError::InternalServerError)?;

    // DELETE is idempotent: always return 204 even if file didn't exist
    Ok(StatusCode::NO_CONTENT)
}
