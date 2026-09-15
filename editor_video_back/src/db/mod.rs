use crate::error::AppError;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Fetches a random background video file path from the `background_videos` table or disk.
/// Validates that candidate paths exist on disk, removing non-existent paths from DB.
/// If no valid path is found in DB, scans `/app/media/backgrounds` (and `./media/backgrounds`),
/// populates DB with discovered video files, and returns a valid path.
pub async fn get_random_background(pool: &PgPool) -> Result<String, AppError> {
    if let Ok(db_paths) = sqlx::query_scalar::<_, String>(
        "SELECT file_path FROM background_videos ORDER BY RANDOM()",
    )
    .fetch_all(pool)
    .await
    {
        for path in db_paths {
            if Path::new(&path).exists() {
                return Ok(path);
            } else {
                warn!(
                    "Background video in DB does not exist on disk: {}. Removing invalid entry.",
                    path
                );
                let _ = sqlx::query("DELETE FROM background_videos WHERE file_path = $1")
                    .bind(&path)
                    .execute(pool)
                    .await;
            }
        }
    }

    let candidate_dirs = [
        PathBuf::from("/app/media/backgrounds"),
        PathBuf::from("./media/backgrounds"),
        PathBuf::from("media/backgrounds"),
    ];

    let mut found_files = Vec::new();
    for dir in &candidate_dirs {
        if dir.exists() && dir.is_dir() {
            if let Ok(mut read_dir) = tokio::fs::read_dir(dir).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            let ext_lower = ext.to_lowercase();
                            if matches!(ext_lower.as_str(), "mp4" | "mov" | "mkv" | "avi" | "webm") {
                                if let Some(path_str) = path.to_str() {
                                    found_files.push(path_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
            if !found_files.is_empty() {
                break;
            }
        }
    }

    if found_files.is_empty() {
        return Err(AppError::Validation(
            "No background videos found in DB or /app/media/backgrounds directory. Please add a video file (e.g. .mp4) to media/backgrounds/".to_string(),
        ));
    }

    for file in &found_files {
        let _ = sqlx::query(
            "INSERT INTO background_videos (file_path) SELECT $1 WHERE NOT EXISTS (SELECT 1 FROM background_videos WHERE file_path = $1)",
        )
        .bind(file)
        .execute(pool)
        .await;
    }

    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    let selected_index = nanos % found_files.len();
    let selected = found_files[selected_index].clone();

    info!("Selected background video from disk scan: {}", selected);
    Ok(selected)
}


