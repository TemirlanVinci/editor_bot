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
            .map_err(|e| AppError::Internal(format!("Failed to create zip file: {}", e)))?;

        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for file_path in files {
            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| AppError::Internal("Invalid file name".to_string()))?;

            zip.start_file(file_name, options).map_err(|e| {
                AppError::Internal(format!("Failed to start zip file {}: {}", file_name, e))
            })?;

            let mut f = File::open(&file_path)
                .map_err(|e| AppError::Internal(format!("Failed to open fragment file: {}", e)))?;

            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)
                .map_err(|e| AppError::Internal(format!("Failed to read fragment file: {}", e)))?;

            zip.write_all(&buffer).map_err(|e| {
                AppError::Internal(format!("Failed to write fragment to zip: {}", e))
            })?;
        }

        zip.finish()
            .map_err(|e| AppError::Internal(format!("Failed to finish zip: {}", e)))?;

        Ok::<(), AppError>(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Zip task panicked: {}", e)))??;

    Ok(())
}

/// Склеивает два готовых видеофайла (например, вступительный хук и тело истории)
/// через FFmpeg filter_complex concat без рассинхрона видео и аудио.
pub async fn concat_video_segments(
    part_a_path: &Path,
    part_b_path: &Path,
    output_path: &Path,
) -> Result<(), AppError> {
    tracing::info!(
        "🔗 Concatenating video segments {:?} and {:?} via filter_complex...",
        part_a_path,
        part_b_path
    );

    let output = tokio::process::Command::new("ffmpeg")
        .stdin(std::process::Stdio::null())
        .args(["-nostdin", "-y"])
        .args(["-i"])
        .arg(part_a_path)
        .args(["-i"])
        .arg(part_b_path)
        .args([
            "-filter_complex",
            "[0:v][0:a][1:v][1:a]concat=n=2:v=1:a=1[outv][outa]",
            "-map",
            "[outv]",
            "-map",
            "[outa]",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-c:a",
            "aac",
        ])
        .arg(output_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to execute ffmpeg video concat: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!("FFmpeg video concat error: {}", stderr);
        return Err(AppError::Internal(format!(
            "FFmpeg video concat failed: {}",
            stderr
        )));
    }

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
    include_intro: bool,
) -> Result<PathBuf, AppError> {
    // 1. Извлекаем полную аудиодорожку и определяем общую длительность видео
    tracing::info!("⚙️ [Step 1/6] Extracting full audio from source video...");
    let full_audio_path = temp_dir.join("full_audio.m4a");
    crate::services::video::audio::extract_full_audio(input_path, &full_audio_path).await?;
    let total_duration = crate::services::video::audio::get_video_duration(input_path).await?;

    // 2. Распознавание речи через Whisper (полный транскрипт с пословными таймкодами и паузами)
    tracing::info!(
        "⚙️ [Step 2/6] Transcribing complete audio track with Whisper (duration: {:.2}s)...",
        total_duration
    );
    let whisper_ctx = crate::services::video::subtitles::get_context()?;
    let language = crate::services::video::subtitles::default_language();
    let transcript = crate::services::video::subtitles::transcribe_full_audio(
        whisper_ctx,
        &full_audio_path,
        &language,
        total_duration,
    )
    .await?;

    // 3. Анализ нарративного контекста с помощью Qwen (единый вступительный вопрос + истории)
    tracing::info!(
        "⚙️ [Step 3/6] Analyzing narrative boundaries with Qwen (global intro + stories)..."
    );
    let segmentation = crate::services::video::segmentation::analyze_narrative(&transcript).await?;

    // Формируем планы монтажа клипов: каждый клип состоит из [Интро-заголовок] + [Тело истории] (если include_intro = true)
    // либо только [Тело истории] (если include_intro = false)
    let clip_plans = segmentation.build_clip_plans(include_intro);

    if clip_plans.is_empty() {
        return Err(AppError::Internal(
            "No valid stories could be extracted from video transcription".to_string(),
        ));
    }

    // Сохраняем segments.json во временной директории (попадает в ZIP и используется /reddit_acc)
    let segments_json_path = temp_dir.join("segments.json");
    if let Ok(json_str) = serde_json::to_string_pretty(&segmentation) {
        let _ = tokio::fs::write(&segments_json_path, json_str).await;
    }

    // 4. Сборка аудиофрагментов клипов: склейка Global Intro + Story или прямая нарезка без интро
    let (audio_paths, segment_words) = if include_intro {
        tracing::info!(
            "⚙️ [Step 4/6] Dynamically assembling {} clip audio track(s) with intro-hook...",
            clip_plans.len()
        );
        let audio_paths = crate::services::video::audio::prepare_clip_audio_tracks(
            &full_audio_path,
            &clip_plans,
            temp_dir,
        )
        .await?;

        // Формируем пословные таймкоды для каждого клипа: слова интро + сдвинутые слова истории (с учетом паузы)
        let pause_dur = crate::services::video::segmentation::intro_pause_duration();
        let segment_words: Vec<Vec<crate::services::video::subtitles::TimedWord>> = clip_plans
            .iter()
            .map(|plan| {
                let intro_val = plan.intro.as_ref().unwrap();
                let intro_words = crate::services::video::subtitles::slice_words_for_interval(
                    &transcript.words,
                    intro_val.start_timestamp,
                    intro_val.end_timestamp,
                );
                let intro_dur = (intro_val.end_timestamp - intro_val.start_timestamp).max(0.0);

                let story_words = crate::services::video::subtitles::slice_words_for_interval(
                    &transcript.words,
                    plan.story.start_timestamp,
                    plan.story.end_timestamp,
                );

                let shift = intro_dur + pause_dur;
                let mut combined = intro_words;
                for mut w in story_words {
                    w.start += shift;
                    w.end += shift;
                    combined.push(w);
                }
                combined
            })
            .collect();

        (audio_paths, segment_words)
    } else {
        tracing::info!(
            "⚙️ [Step 4/6] Slicing {} clip audio track(s) directly without intro concatenation...",
            clip_plans.len()
        );
        let audio_paths = crate::services::video::audio::prepare_story_audio_tracks(
            &full_audio_path,
            &clip_plans,
            temp_dir,
        )
        .await?;

        // Формируем пословные таймкоды для каждого клипа: только слова истории без интро
        let segment_words: Vec<Vec<crate::services::video::subtitles::TimedWord>> = clip_plans
            .iter()
            .map(|plan| {
                crate::services::video::subtitles::slice_words_for_interval(
                    &transcript.words,
                    plan.story.start_timestamp,
                    plan.story.end_timestamp,
                )
            })
            .collect();

        (audio_paths, segment_words)
    };

    // Удаляем исходное видео и промежуточную полную аудиодорожку для экономии дискового пространства
    let _ = tokio::fs::remove_file(input_path).await;
    let _ = tokio::fs::remove_file(&full_audio_path).await;

    // 5. Вычисляем общую необходимую длительность фонового видео для всех клипов
    let mut total_required_duration: f64 = 0.0;
    for plan in &clip_plans {
        let dur = plan.total_duration.max(1.0);
        total_required_duration += dur / crate::services::video::render::SPEED_FACTOR;
    }

    tracing::info!(
        "⚙️ [Step 5/6] Preparing random background video sequence (required duration: {:.2}s) and music...",
        total_required_duration
    );
    let background_path = crate::services::video::background::prepare_background_sequence(
        pool,
        total_required_duration,
        temp_dir,
    )
    .await?;

    let music_path = crate::services::video::music::get_random_music(&music_dir()).await?;

    // 6. Рендеринг финальных видеоклипов с караоке-субтитрами и фоном
    tracing::info!(
        "⚙️ [Step 6/6] Rendering {} assembled video clip(s) with anti-fraud mutations...",
        audio_paths.len()
    );
    let mut files_to_zip = crate::services::video::render::render_narrative_fragments(
        audio_paths,
        segment_words,
        &background_path,
        &music_path,
        temp_dir,
    )
    .await?;

    // Добавляем segments.json в ZIP-архив для downstream потребителей
    if tokio::fs::try_exists(&segments_json_path)
        .await
        .unwrap_or(false)
    {
        files_to_zip.push(segments_json_path);
    }

    let zip_path = temp_dir.join("fragments.zip");
    create_zip(files_to_zip, zip_path.clone()).await?;

    tracing::info!(
        "✅ Narrative segmentation pipeline complete! Returning ZIP archive: {:?}",
        zip_path
    );
    Ok(zip_path)
}
