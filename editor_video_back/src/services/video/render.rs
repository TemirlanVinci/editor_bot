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

/// Громкость фоновой музыки относительно голоса (0.0 - 1.0).
/// 0.12-0.18 — музыка слышна, но не перебивает голос.
const MUSIC_VOLUME: f64 = 0.15;

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
        let bg_path = background_path.to_path_buf();
        let music_p = music_path.to_path_buf();
        let sem = semaphore.clone();

        let task = tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.map_err(|e| {
                AppError::Validation(format!("Failed to acquire semaphore permit: {}", e))
            })?;

            info!(
                "🚀 Rendering fragment {}/{}...",
                fragment_index, total_fragments
            );
            let start_time = std::time::Instant::now();

            let audio_dur = crate::services::video::audio::get_video_duration(&audio_path).await?;
            // Изменение 1: Длительность под 1.08x (8% ускорения)
            let target_duration = audio_dur / 1.08;

            let output = Command::new("ffmpeg")
                .stdin(Stdio::null())
                .args(["-nostdin", "-y", "-stream_loop", "-1", "-i"])
                .arg(&bg_path)
                .arg("-i")
                .arg(&audio_path)
                .args([
                    "-filter_complex",
                    // Изменение 2 и 3: setpts = 1 / 1.08 (0.925926), atempo = 1.08
                    "[0:v]setpts=0.925926*PTS[v];[1:a]atempo=1.08[a]",
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
