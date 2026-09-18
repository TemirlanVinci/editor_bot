use crate::error::AppError;
use rand::seq::SliceRandom;
use std::path::{Path, PathBuf};
use tokio::fs;

const MUSIC_EXTENSIONS: [&str; 4] = ["mp3", "m4a", "wav", "aac"];

/// Выбирает случайный аудиофайл из директории с фоновой музыкой.
pub async fn get_random_music(music_dir: &Path) -> Result<PathBuf, AppError> {
    let mut entries = fs::read_dir(music_dir).await.map_err(|e| {
        AppError::Validation(format!(
            "Failed to read music directory {:?}: {}",
            music_dir, e
        ))
    })?;

    let mut candidates = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to read music directory entry: {}", e)))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|e| AppError::Validation(format!("Failed to read music file type: {}", e)))?;

        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && MUSIC_EXTENSIONS.contains(&ext.to_lowercase().as_str())
        {
            candidates.push(path);
        }
    }

    if candidates.is_empty() {
        return Err(AppError::Validation(format!(
            "No music files found in directory {:?}",
            music_dir
        )));
    }

    let mut rng = rand::thread_rng();
    candidates
        .choose(&mut rng)
        .cloned()
        .ok_or_else(|| AppError::Validation("Failed to select random music track".to_string()))
}
