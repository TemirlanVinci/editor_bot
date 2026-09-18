use crate::error::AppError;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{error, info};

pub async fn download_youtube_video(url: &str, temp_dir: &Path) -> Result<PathBuf, AppError> {
    info!(url = %url, temp_dir = ?temp_dir, "Starting yt-dlp video download");

    let output_path = temp_dir.join("video.mp4");
    let output_str = output_path
        .to_str()
        .ok_or_else(|| AppError::Internal("Invalid output path string".to_string()))?;

    let output = Command::new("yt-dlp")
        .args([
            "-f",
            "bestvideo[height<=1080]+bestaudio/best[height<=1080]",
            "--merge-output-format",
            "mp4",
            "--no-playlist",
            "-o",
            output_str,
            url,
        ])
        .output()
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to execute yt-dlp process");
            AppError::Internal(format!("Failed to execute yt-dlp: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(status = ?output.status, stderr = %stderr, "yt-dlp download failed");
        return Err(AppError::Internal(format!("yt-dlp failed: {}", stderr)));
    }

    if output_path.exists() {
        info!(
            "✅ yt-dlp video download completed successfully at {:?}",
            output_path
        );
        return Ok(output_path);
    }

    let mut entries = tokio::fs::read_dir(temp_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read temp directory: {}", e)))?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("mp4") {
            info!("✅ Found downloaded mp4 file at {:?}", path);
            return Ok(path);
        }
    }

    Err(AppError::Internal(
        "yt-dlp finished but output video file was not found".to_string(),
    ))
}
