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
        if dir.exists() && dir.is_dir() {
            if let Ok(mut read_dir) = tokio::fs::read_dir(dir).await {
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    let path = entry.path();
                    if path.is_file()
                        && let Some(ext) = path.extension().and_then(|e| e.to_str())
                    {
                        let ext_lower = ext.to_lowercase();
                        if matches!(ext_lower.as_str(), "mp4" | "mov" | "mkv" | "avi" | "webm")
                            && let Some(path_str) = path.to_str()
                        {
                            if !found_files.contains(&path_str.to_string()) {
                                found_files.push(path_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    found_files
}

/// Выбирает случайный путь к фоновому видео файлу.
/// Сканирует директории (`media/background`, `media/backgrounds` и др.),
/// синхронизирует найденное с базой данных `background_videos` (добавляет новые, удаляет несуществующие),
/// и возвращает случайно выбранное фоновое видео.
pub async fn get_random_background(pool: &PgPool) -> Result<String, AppError> {
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
    if let Ok(db_paths) =
        sqlx::query_scalar::<_, String>("SELECT file_path FROM background_videos")
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

    // 3. Выбираем случайное видео из БД (которая синхронизирована с диском)
    if let Ok(db_paths) =
        sqlx::query_scalar::<_, String>("SELECT file_path FROM background_videos ORDER BY RANDOM()")
            .fetch_all(pool)
            .await
    {
        for path in db_paths {
            if Path::new(&path).exists() {
                info!("Selected random background video: {}", path);
                return Ok(path);
            }
        }
    }

    // 4. Запасной вариант: берем случайный файл напрямую из скан-списка
    if !disk_files.is_empty() {
        let mut rng = rand::thread_rng();
        if let Some(selected) = disk_files.choose(&mut rng) {
            info!("Selected background video from disk scan fallback: {}", selected);
            return Ok(selected.clone());
        }
    }

    Err(AppError::Validation(
        "No background videos found. Please add video files (e.g. .mp4) to media/background or media/backgrounds directory.".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_background_candidate_dirs_includes_background_and_backgrounds() {
        let dirs = get_background_candidate_dirs();
        let dirs_str: Vec<String> = dirs.iter().map(|d| d.to_string_lossy().to_string()).collect();

        assert!(dirs_str.iter().any(|d| d.contains("background")));
        assert!(dirs_str.iter().any(|d| d.contains("backgrounds")));
    }
}

