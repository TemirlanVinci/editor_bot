pub mod tiktok;

use crate::error::AppError;

use rand::seq::SliceRandom;
use sqlx::PgPool;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Возвращает список директорий для поиска фоновых видео на основе переменных окружения и путей по умолчанию.
fn get_background_candidate_dirs() -> Vec<PathBuf> {
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

/// Сканирует диск в поисках подходящих видеофайлов в фоновых директориях.
async fn scan_background_files_from_disk() -> Vec<String> {
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

/// Возвращает список всех доступных фоновых видео (из базы данных или диска).
pub async fn get_available_backgrounds(pool: &PgPool) -> Result<Vec<String>, AppError> {
    // 1. Сканируем актуальные видеофайлы с диска
    let disk_files = scan_background_files_from_disk().await;

    // 2. Если файлы на диске найдены, вставляем новые в БД
    if !disk_files.is_empty() {
        for file in &disk_files {
            let _ = sqlx::query(
                "INSERT INTO background_videos (file_path) SELECT $1 WHERE NOT EXISTS (SELECT 1 FROM background_videos WHERE file_path = $1)",
            )
            .bind(file)
            .execute(pool)
            .await;
        }
    }

    // Удаляем из БД записи о файлах, которых больше нет на диске
    if let Ok(db_paths) = sqlx::query_scalar::<_, String>("SELECT file_path FROM background_videos")
        .fetch_all(pool)
        .await
    {
        for path in db_paths {
            if !Path::new(&path).exists() {
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

    // 3. Возвращаем имеющиеся видео из БД
    if let Ok(valid_db_paths) =
        sqlx::query_scalar::<_, String>("SELECT file_path FROM background_videos")
            .fetch_all(pool)
            .await
    {
        let existing: Vec<String> = valid_db_paths
            .into_iter()
            .filter(|p| Path::new(p).exists())
            .collect();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }

    // 4. Запасной вариант: берем список напрямую из скан-списка диска
    if !disk_files.is_empty() {
        return Ok(disk_files);
    }

    Err(AppError::Validation(
        "No background videos found. Please add video files (e.g. .mp4) to media/background or media/backgrounds directory.".to_string(),
    ))
}

/// Выбирает случайный путь к фоновому видео файлу.
pub async fn get_random_background(pool: &PgPool) -> Result<String, AppError> {
    let files = get_available_backgrounds(pool).await?;
    let mut rng = rand::thread_rng();
    if let Some(selected) = files.choose(&mut rng) {
        info!("Selected random background video: {}", selected);
        return Ok(selected.clone());
    }

    Err(AppError::Validation(
        "No background videos found. Please add video files (e.g. .mp4) to media/background or media/backgrounds directory.".to_string(),
    ))
}

/// Формирует цепочку случайных фоновых видео, последовательно добавляя их,
/// пока суммарная длительность фонового видео не станет >= required_duration.
/// Если получена цепочка из нескольких видео, склеивает их в единый файл в temp_dir.
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
        "🎯 Building background video sequence to cover required duration {:.2}s...",
        required_duration
    );

    while total_duration < required_duration {
        let candidates: Vec<&String> = if available.len() > 1 {
            available
                .iter()
                .filter(|&p| last_selected.as_ref() != Some(p))
                .collect()
        } else {
            available.iter().collect()
        };

        let chosen: &String = candidates
            .choose(&mut rand::thread_rng())
            .copied()
            .unwrap_or_else(|| available.choose(&mut rand::thread_rng()).unwrap());

        let chosen_path = PathBuf::from(chosen);
        let dur = crate::services::video::audio::get_video_duration(&chosen_path).await?;

        info!(
            "➕ Selected background video {:?} (duration: {:.2}s, accumulated: {:.2}s / required: {:.2}s)",
            chosen_path,
            dur,
            total_duration + dur,
            required_duration
        );

        total_duration += dur;
        selected_files.push(chosen_path);
        last_selected = Some((*chosen).clone());
    }

    if selected_files.len() == 1 {
        info!(
            "Single background video is long enough ({:.2}s >= {:.2}s). Using directly: {:?}",
            total_duration, required_duration, selected_files[0]
        );
        return Ok(selected_files[0].clone());
    }

    let concat_output = temp_dir.join("combined_background.mp4");
    crate::services::video::render::concat_background_videos(&selected_files, &concat_output).await?;

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
