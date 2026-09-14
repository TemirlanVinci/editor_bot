use axum::{
    extract::Multipart,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::io::Read;
use tempfile::tempdir;
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;
use crate::error::AppError;

pub async fn cut_video(mut multipart: Multipart) -> Result<Response, AppError> {
    let mut video_bytes = None;
    
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to parse multipart: {}", e)))?
    {
        if field.name() == Some("video") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::Validation(format!("Failed to read video field: {}", e)))?;
            video_bytes = Some(bytes);
            break;
        }
    }

    let bytes = video_bytes.ok_or_else(|| AppError::Validation("Missing 'video' field".to_string()))?;

    // Create a temporary directory that will be deleted when dropped
    let temp_dir = tempdir().map_err(|e| AppError::Validation(format!("Failed to create temp dir: {}", e)))?;
    let input_path = temp_dir.path().join("input.mp4");

    let mut file = TokioFile::create(&input_path)
        .await
        .map_err(|e| AppError::Validation(format!("Failed to create input video file: {}", e)))?;
    
    file.write_all(&bytes)
        .await
        .map_err(|e| AppError::Validation(format!("Failed to write input video: {}", e)))?;

    // Process the video using FFmpeg and create a zip file
    let zip_path = crate::services::video::cut::process_video(&input_path, temp_dir.path()).await?;

    // Read the zip file into memory before dropping the temporary directory
    let mut zip_file = std::fs::File::open(&zip_path)
        .map_err(|e| AppError::Validation(format!("Failed to open generated zip file: {}", e)))?;
    
    let mut zip_data = Vec::new();
    zip_file.read_to_end(&mut zip_data)
        .map_err(|e| AppError::Validation(format!("Failed to read generated zip file: {}", e)))?;

    // Drop temp_dir to explicitly clean up temporary files (input and fragments)
    drop(temp_dir);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/zip")],
        zip_data,
    ).into_response())
}
