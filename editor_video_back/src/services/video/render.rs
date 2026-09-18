use crate::error::AppError;
use futures_util::future::join_all;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;
use tracing::error;

use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;

use crate::services::video::{audio, subtitles};

/// Громкость фоновой музыки относительно голоса (0.0 - 1.0).
/// 0.20-0.30 — музыка слышна, но не перебивает голос.
pub const MUSIC_VOLUME: f64 = 0.22;

/// Во сколько раз ускоряется видео/аудио при рендере (анти-фрод мутация).
/// Субтитры тоже масштабируются этим коэффициентом, чтобы не разъезжаться
/// с озвучкой — см. subtitles::build_karaoke_ass.
pub const SPEED_FACTOR: f64 = 1.08;

pub async fn render_fragments(
    audio_paths: Vec<PathBuf>,
    background_path: &Path,
    music_path: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    let total_fragments = audio_paths.len();
    info!(
        "🎬 Starting parallel rendering for {} fragment(s) with background {:?} and music {:?}",
        total_fragments, background_path, music_path
    );

    if total_fragments == 0 {
        return Ok(Vec::new());
    }

    // Модель Whisper грузится один раз на процесс (см. subtitles::get_context)
    // и переиспользуется для всех фрагментов и всех запросов.
    info!("📝 Preparing subtitle engine...");
    let whisper_ctx = subtitles::get_context()?;
    let subtitle_language = subtitles::default_language();

    // PlayResX/PlayResY субтитров должны совпадать с реальным разрешением
    // фонового видео, иначе караоке-текст съедет при другом соотношении сторон.
    let (bg_width, bg_height) = audio::get_video_dimensions(background_path).await?;

    let max_concurrency = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .min(4);
    let semaphore = Arc::new(Semaphore::new(max_concurrency));

    let mut tasks = Vec::with_capacity(total_fragments);

    for (index, audio_path) in audio_paths.into_iter().enumerate() {
        let fragment_index = index + 1;
        let output_filename = format!("fragment_{}_final.mp4", fragment_index);
        let output_path = output_dir.join(output_filename);
        let ass_path = output_dir.join(format!("fragment_{}_subs.ass", fragment_index));
        let bg_path = background_path.to_path_buf();
        let music_p = music_path.to_path_buf();
        let sem = semaphore.clone();
        let ctx = whisper_ctx.clone();
        let language = subtitle_language.clone();

        let task = tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.map_err(|e| {
                AppError::Validation(format!("Failed to acquire semaphore permit: {}", e))
            })?;

            info!(
                "🚀 Rendering fragment {}/{}...",
                fragment_index, total_fragments
            );
            let start_time = std::time::Instant::now();

            let audio_dur = audio::get_video_duration(&audio_path).await?;
            // Изменение 1: Длительность под 1.08x (8% ускорения)
            let target_duration = audio_dur / SPEED_FACTOR;

            info!(
                "🗣️ Transcribing fragment {}/{} for karaoke subtitles...",
                fragment_index, total_fragments
            );
            let mut words = subtitles::transcribe_words(ctx, &audio_path, &language).await?;

            // Очищаем каждое слово от знаков препинания по краям (точки, запятые, кавычки и т.д.)
            for w in &mut words {
                w.text = w
                    .text
                    .trim_matches(|c: char| {
                        c.is_ascii_punctuation() || matches!(c, '—' | '…' | '«' | '»' | '“' | '”')
                    })
                    .to_string();
            }

            let ass_content =
                subtitles::build_karaoke_ass(&words, bg_width, bg_height, SPEED_FACTOR);
            fs::write(&ass_path, ass_content).await.map_err(|e| {
                AppError::Validation(format!("Failed to write subtitle file: {}", e))
            })?;

            // Изменение 2 и 3: setpts = 1 / 1.08 (0.925926), atempo = 1.08
            // + прожиг караоке-субтитров фильтром ass сразу после setpts.
            // + наложение фоновой музыки с громкостью MUSIC_VOLUME.
            let filter_complex = format!(
                "[0:v]setpts={setpts:.6}*PTS,ass='{ass}'[v];[1:a]atempo={speed}[voice];[2:a]volume={volume}[music];[voice][music]amix=inputs=2:duration=first:dropout_transition=2:normalize=0[a]",
                setpts = 1.0 / SPEED_FACTOR,
                ass = subtitles::escape_ffmpeg_path(&ass_path),
                speed = SPEED_FACTOR,
                volume = MUSIC_VOLUME
            );

            let output = Command::new("ffmpeg")
                .stdin(Stdio::null())
                .args(["-nostdin", "-y", "-stream_loop", "-1", "-i"])
                .arg(&bg_path)
                .arg("-i")
                .arg(&audio_path)
                .args(["-stream_loop", "-1", "-i"])
                .arg(&music_p)
                .args(["-filter_complex", &filter_complex])
                .args([
                    "-map",
                    "[v]",
                    "-map",
                    "[a]",
                    "-t",
                    &format!("{:.3}", target_duration),
                    "-map_metadata",
                    "-1",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-c:a",
                    "aac",
                ])
                .arg(&output_path)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| AppError::Validation(format!("Failed to execute ffmpeg: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "FFmpeg rendering error for fragment {}: {}",
                    fragment_index, stderr
                );
                return Err(AppError::Validation(format!(
                    "FFmpeg render failed for fragment {}: {}",
                    fragment_index, stderr
                )));
            }

            if let Err(e) = fs::remove_file(&audio_path).await {
                error!(
                    "Failed to remove temporary audio file {:?} for fragment {}: {}",
                    audio_path, fragment_index, e
                );
                return Err(AppError::Validation(format!(
                    "Failed to delete temporary audio file: {}",
                    e
                )));
            }

            // .ass-файл лежит во временной директории запроса и в любом случае
            // будет удалён вместе с ней, но подчищаем сразу, чтобы не копился
            // мусор при долгих запросах с большим числом фрагментов.
            let _ = fs::remove_file(&ass_path).await;

            info!(
                "✅ Fragment {}/{} rendered in {:.2}s",
                fragment_index,
                total_fragments,
                start_time.elapsed().as_secs_f32()
            );

            Ok((index, output_path))
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;
    let mut indexed_files = Vec::with_capacity(results.len());

    for result in results {
        match result {
            Ok(Ok((index, path))) => {
                indexed_files.push((index, path));
            }
            Ok(Err(err)) => {
                return Err(err);
            }
            Err(join_err) => {
                error!("Render task panicked: {}", join_err);
                return Err(AppError::Validation(format!(
                    "Render task failed: {}",
                    join_err
                )));
            }
        }
    }

    indexed_files.sort_by_key(|(index, _)| *index);

    let rendered_files = indexed_files.into_iter().map(|(_, path)| path).collect();

    info!(
        "🎉 All {} fragment(s) rendered successfully!",
        total_fragments
    );

    Ok(rendered_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_render_fragments_empty() {
        let temp = tempdir().expect("Failed to create temp dir");
        let bg_path = temp.path().join("bg.mp4");
        let music_path = temp.path().join("music.mp3");
        let result = render_fragments(vec![], &bg_path, &music_path, temp.path()).await;
        assert!(result.is_ok());
        assert!(result.expect("Expected Ok").is_empty());
    }
}
