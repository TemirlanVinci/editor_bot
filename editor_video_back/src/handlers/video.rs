use crate::error::AppError;
use crate::models::video::DownloadVideoRequest;
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Multipart, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::stream::Stream;
use sqlx::PgPool;
use std::pin::Pin;
use std::task::{Context, Poll};
use tempfile::{TempDir, tempdir};
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::info;
use validator::Validate;

struct CleanupStream {
    stream: ReaderStream<TokioFile>,
    _temp_dir: TempDir,
}

impl Stream for CleanupStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

pub async fn cut_video(
    State(pool): State<PgPool>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    info!("📥 Received cut_video request");
    let mut temp_dir = None;
    let mut input_path = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to parse multipart: {}", e)))?
    {
        if field.name() == Some("video") {
            let dir = tempdir()
                .map_err(|e| AppError::Validation(format!("Failed to create temp dir: {}", e)))?;
            let file_path = dir.path().join("input.mp4");

            let mut file = TokioFile::create(&file_path).await.map_err(|e| {
                AppError::Validation(format!("Failed to create input video file: {}", e))
            })?;

            let mut bytes_written = 0u64;
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::Validation(format!("Failed to read video chunk: {}", e)))?
            {
                bytes_written += chunk.len() as u64;
                file.write_all(&chunk).await.map_err(|e| {
                    AppError::Validation(format!("Failed to write input video: {}", e))
                })?;
            }

            info!(
                "📥 Received input video upload ({} bytes saved to temporary file)",
                bytes_written
            );

            temp_dir = Some(dir);
            input_path = Some(file_path);
            break;
        }
    }

    let temp_dir =
        temp_dir.ok_or_else(|| AppError::Validation("Missing 'video' field".to_string()))?;
    let input_path =
        input_path.ok_or_else(|| AppError::Validation("Missing 'video' field".to_string()))?;

    // Process the video using FFmpeg and create a zip file
    let zip_path =
        crate::services::video::cut::process_video(&input_path, temp_dir.path(), &pool).await?;

    let zip_file = TokioFile::open(&zip_path)
        .await
        .map_err(|e| AppError::Validation(format!("Failed to open generated zip file: {}", e)))?;

    info!("📤 Streaming ZIP archive response to client...");

    let reader_stream = ReaderStream::new(zip_file);
    let cleanup_stream = CleanupStream {
        stream: reader_stream,
        _temp_dir: temp_dir,
    };

    let body = Body::from_stream(cleanup_stream);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/zip")],
        body,
    )
        .into_response())
}

pub async fn download_video(
    Json(payload): Json<DownloadVideoRequest>,
) -> Result<Response, AppError> {
    info!(url = %payload.url, "📥 Received download_video request");
    payload.validate()?;

    let temp_dir = tempdir()
        .map_err(|e| AppError::Internal(format!("Failed to create temporary directory: {}", e)))?;

    let downloaded_file_path =
        crate::services::video::download::download_youtube_video(&payload.url, temp_dir.path())
            .await?;

    let video_file = TokioFile::open(&downloaded_file_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to open downloaded video file: {}", e)))?;

    info!("📤 Streaming MP4 video response to client...");

    let reader_stream = ReaderStream::new(video_file);
    let cleanup_stream = CleanupStream {
        stream: reader_stream,
        _temp_dir: temp_dir,
    };

    let body = Body::from_stream(cleanup_stream);

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "video/mp4")], body).into_response())
}
