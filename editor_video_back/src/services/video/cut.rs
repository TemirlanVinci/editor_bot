use crate::error::AppError;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

pub async fn create_zip(files: Vec<PathBuf>, output_zip_path: PathBuf) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let file = File::create(&output_zip_path)
            .map_err(|e| AppError::Validation(format!("Failed to create zip file: {}", e)))?;

        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for file_path in files {
            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| AppError::Validation("Invalid file name".to_string()))?;

            zip.start_file(file_name, options).map_err(|e| {
                AppError::Validation(format!("Failed to start zip file {}: {}", file_name, e))
            })?;

            let mut f = File::open(&file_path).map_err(|e| {
                AppError::Validation(format!("Failed to open fragment file: {}", e))
            })?;

            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer).map_err(|e| {
                AppError::Validation(format!("Failed to read fragment file: {}", e))
            })?;

            zip.write_all(&buffer).map_err(|e| {
                AppError::Validation(format!("Failed to write fragment to zip: {}", e))
            })?;
        }

        zip.finish()
            .map_err(|e| AppError::Validation(format!("Failed to finish zip: {}", e)))?;

        Ok::<(), AppError>(())
    })
    .await
    .map_err(|e| AppError::Validation(format!("Zip task panicked: {}", e)))??;

    Ok(())
}

/// Директория с фоновыми музыкальными треками.
/// По умолчанию — подпапка "music" внутри общей media-директории.
/// Переопределяется через переменные окружения MUSIC_DIR или MEDIA_DIR.
fn music_dir() -> PathBuf {
    if let Ok(dir) = env::var("MUSIC_DIR") {
        return PathBuf::from(dir);
    }

    let media_dir = env::var("MEDIA_DIR").unwrap_or_else(|_| "media".to_string());
    Path::new(&media_dir).join("music")
}

pub async fn process_video(
    input_path: &Path,
    temp_dir: &Path,
    pool: &sqlx::PgPool,
) -> Result<PathBuf, AppError> {
    tracing::info!("⚙️ [Step 1/5] Extracting and splitting audio fragments...");
    let audio_paths =
        crate::services::video::audio::extract_and_split_audio(input_path, temp_dir).await?;

    // Вычисляем общую необходимую длительность фонового видео для всех фрагментов
    let mut total_required_duration: f64 = 0.0;
    for audio_path in &audio_paths {
        let dur = crate::services::video::audio::get_video_duration(audio_path).await?;
        total_required_duration += dur / crate::services::video::render::SPEED_FACTOR;
    }

    tracing::info!(
        "⚙️ [Step 2/5] Preparing random background video sequence (required duration: {:.2}s)...",
        total_required_duration
    );
    let background_path =
        crate::db::prepare_background_sequence(pool, total_required_duration, temp_dir).await?;

    tracing::info!("⚙️ [Step 3/5] Picking random background music track...");
    let music_path = crate::services::video::music::get_random_music(&music_dir()).await?;

    tracing::info!("⚙️ [Step 4/5] Rendering video fragments with anti-fraud mutations...");
    let video_paths = crate::services::video::render::render_fragments(
        audio_paths,
        &background_path,
        &music_path,
        temp_dir,
    )
    .await?;

    tracing::info!("⚙️ [Step 5/5] Archiving final video fragments into ZIP file...");
    let zip_path = temp_dir.join("fragments.zip");
    create_zip(video_paths, zip_path.clone()).await?;

    tracing::info!("✅ Process complete! Returning ZIP archive: {:?}", zip_path);
    Ok(zip_path)
}
