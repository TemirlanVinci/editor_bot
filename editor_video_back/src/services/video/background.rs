use crate::db::background as db_background;
use crate::error::AppError;
use rand::seq::SliceRandom;
use sqlx::PgPool;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Returns list of candidate directories to search for background videos.
pub fn get_background_candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(dir) = env::var("BACKGROUND_DIR") {
        dirs.push(PathBuf::from(dir));
    }

    let media_dir = env::var("MEDIA_DIR").unwrap_or_else(|_| "media".to_string());
    dirs.push(Path::new(&media_dir).join("background"));
    dirs.push(Path::new(&media_dir).join("backgrounds"));

    dirs.push(PathBuf::from("/app/media/background"));
    dirs.push(PathBuf::from("/app/media/backgrounds"));
    dirs.push(PathBuf::from("./media/background"));
    dirs.push(PathBuf::from("./media/backgrounds"));
    dirs.push(PathBuf::from("media/background"));
    dirs.push(PathBuf::from("media/backgrounds"));

    let mut unique_dirs = Vec::new();
    for d in dirs {
        if !unique_dirs.contains(&d) {
            unique_dirs.push(d);
        }
    }
    unique_dirs
}

/// Asynchronously scans disk for candidate background video files.
pub async fn scan_background_files_from_disk() -> Vec<String> {
    let candidate_dirs = get_background_candidate_dirs();
    let mut found_files = Vec::new();

    for dir in &candidate_dirs {
        if dir.exists()
            && dir.is_dir()
            && let Ok(mut read_dir) = tokio::fs::read_dir(dir).await
        {
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                if path.is_file()
                    && let Some(ext) = path.extension().and_then(|e| e.to_str())
                {
                    let ext_lower = ext.to_lowercase();
                    if matches!(ext_lower.as_str(), "mp4" | "mov" | "mkv" | "avi" | "webm")
                        && let Some(path_str) = path.to_str()
                        && !found_files.contains(&path_str.to_string())
                    {
                        found_files.push(path_str.to_string());
                    }
                }
            }
        }
    }

    found_files
}

/// Returns list of all available background videos (syncing disk state with database).
pub async fn get_available_backgrounds(pool: &PgPool) -> Result<Vec<String>, AppError> {
    // 1. Scan actual video files from disk
    let disk_files = scan_background_files_from_disk().await;

    // 2. Batch insert new disk files into database
    if !disk_files.is_empty() {
        db_background::insert_background_files_batch(pool, &disk_files).await?;
    }

    // 3. Remove DB entries for files that no longer exist on disk (using batch delete)
    if let Ok(db_paths) = db_background::fetch_all_background_paths(pool).await {
        let missing: Vec<String> = db_paths
            .into_iter()
            .filter(|path| !Path::new(path).exists())
            .inspect(|path| {
                warn!(
                    path = %path,
                    "Background video in DB does not exist on disk. Removing invalid entry."
                );
            })
            .collect();

        if !missing.is_empty() {
            db_background::delete_background_files_batch(pool, &missing).await?;
        }
    }

    // 4. Return existing valid DB paths
    if let Ok(valid_db_paths) = db_background::fetch_all_background_paths(pool).await {
        let existing: Vec<String> = valid_db_paths
            .into_iter()
            .filter(|p| Path::new(p).exists())
            .collect();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }

    // 5. Fallback: return disk files directly
    if !disk_files.is_empty() {
        return Ok(disk_files);
    }

    Err(AppError::Validation(
        "No background videos found. Please add video files (e.g. .mp4) to media/background or media/backgrounds directory.".to_string(),
    ))
}

/// Picks a random background video file path safely without unwrap.
pub async fn get_random_background(pool: &PgPool) -> Result<String, AppError> {
    let files = get_available_backgrounds(pool).await?;
    let selected = {
        let mut rng = rand::thread_rng();
        files.choose(&mut rng).cloned()
    };

    if let Some(selected) = selected {
        info!(video = %selected, "Selected random background video");
        return Ok(selected);
    }

    Err(AppError::Validation(
        "No background videos found. Please add video files (e.g. .mp4) to media/background or media/backgrounds directory.".to_string(),
    ))
}

/// Prepares a background video sequence of required duration.
pub async fn prepare_background_sequence(
    pool: &PgPool,
    required_duration: f64,
    temp_dir: &Path,
) -> Result<PathBuf, AppError> {
    let available = get_available_backgrounds(pool).await?;
    if available.is_empty() {
        return Err(AppError::Validation(
            "No background videos found. Please add video files (e.g. .mp4) to media/background or media/backgrounds directory.".to_string(),
        ));
    }

    let mut selected_files: Vec<PathBuf> = Vec::new();
    let mut total_duration = 0.0;
    let mut last_selected: Option<String> = None;

    info!(
        required_duration = %required_duration,
        "Building background video sequence to cover required duration"
    );

    while total_duration < required_duration {
        let chosen = {
            let candidates: Vec<&String> = if available.len() > 1 {
                available
                    .iter()
                    .filter(|&p| last_selected.as_ref() != Some(p))
                    .collect()
            } else {
                available.iter().collect()
            };

            let mut rng = rand::thread_rng();
            candidates
                .choose(&mut rng)
                .copied()
                .or_else(|| available.choose(&mut rng))
                .ok_or_else(|| {
                    AppError::Validation("Failed to pick background video candidate".to_string())
                })?
                .clone()
        };

        let chosen_path = PathBuf::from(&chosen);
        let dur = crate::services::video::audio::get_video_duration(&chosen_path).await?;

        info!(
            chosen_path = ?chosen_path,
            dur = %dur,
            accumulated = %(total_duration + dur),
            required = %required_duration,
            "Added background video to sequence"
        );

        total_duration += dur;
        selected_files.push(chosen_path);
        last_selected = Some(chosen);
    }

    if selected_files.len() == 1 {
        info!(
            total_duration = %total_duration,
            required_duration = %required_duration,
            selected = ?selected_files[0],
            "Single background video is long enough, using directly"
        );
        return Ok(selected_files[0].clone());
    }

    let concat_output = temp_dir.join("combined_background.mp4");
    crate::services::video::render::concat_background_videos(&selected_files, &concat_output)
        .await?;

    Ok(concat_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_background_candidate_dirs_includes_background_and_backgrounds() {
        let dirs = get_background_candidate_dirs();
        let dirs_str: Vec<String> = dirs
            .iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect();

        assert!(dirs_str.iter().any(|d| d.contains("background")));
        assert!(dirs_str.iter().any(|d| d.contains("backgrounds")));
    }
}
