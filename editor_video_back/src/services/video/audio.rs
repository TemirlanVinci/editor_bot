use crate::error::AppError;
use futures_util::future::join_all;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;
use tracing::error;

#[derive(Debug, PartialEq)]
pub struct Interval {
    pub start: f64,
    pub end: f64,
}

pub fn calculate_intervals(duration: f64) -> Vec<Interval> {
    if duration <= 60.0 {
        return vec![Interval {
            start: 0.0,
            end: duration,
        }];
    }

    let mut intervals = Vec::new();
    let mut i = 1;
    loop {
        let start = if i == 1 {
            0.0
        } else {
            ((i - 1) * 60) as f64 - 2.0
        };
        let mut end = (i * 60) as f64;

        if end > duration {
            end = duration;
        }

        let length = end - start;

        if i > 1 && length < 10.0 {
            // Tail is less than 10 seconds, discard it.
            break;
        }

        intervals.push(Interval { start, end });

        if end >= duration {
            break;
        }

        i += 1;
    }

    intervals
}

pub async fn get_video_duration(input_path: &Path) -> Result<f64, AppError> {
    let output = Command::new("ffprobe")
        .stdin(Stdio::null())
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input_path)
        .output()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to execute ffprobe: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("ffprobe error: {}", stderr);
        return Err(AppError::Validation(
            "Failed to determine video duration".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let duration: f64 = stdout
        .trim()
        .parse()
        .map_err(|_| AppError::Validation("Failed to parse video duration".to_string()))?;

    Ok(duration)
}

/// Возвращает (width, height) первого видеопотока файла. Используется, чтобы
/// сгенерировать субтитры с PlayResX/PlayResY, совпадающим с реальным кадром
/// (иначе караоке-текст может уехать при несовпадении соотношения сторон).
pub async fn get_video_dimensions(input_path: &Path) -> Result<(u32, u32), AppError> {
    let output = Command::new("ffprobe")
        .stdin(Stdio::null())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
        ])
        .arg(input_path)
        .output()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to execute ffprobe: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("ffprobe error: {}", stderr);
        return Err(AppError::Validation(
            "Failed to determine video dimensions".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let dims: Vec<&str> = stdout.trim().split('x').collect();
    if dims.len() != 2 {
        return Err(AppError::Validation(
            "Failed to parse video dimensions".to_string(),
        ));
    }

    let width: u32 = dims[0]
        .parse()
        .map_err(|_| AppError::Validation("Failed to parse video width".to_string()))?;
    let height: u32 = dims[1]
        .parse()
        .map_err(|_| AppError::Validation("Failed to parse video height".to_string()))?;

    Ok((width, height))
}

/// Извлекает полную аудиодорожку из исходного видеофайла без видеопотока.
pub async fn extract_full_audio(
    input_video_path: &Path,
    output_audio_path: &Path,
) -> Result<(), AppError> {
    tracing::info!(
        "🎵 Extracting full audio from {:?} to {:?}",
        input_video_path,
        output_audio_path
    );

    let output = Command::new("ffmpeg")
        .stdin(Stdio::null())
        .args(["-nostdin", "-y", "-i"])
        .arg(input_video_path)
        .args(["-vn", "-c:a", "aac"])
        .arg(output_audio_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            AppError::Validation(format!(
                "Failed to execute ffmpeg full audio extract: {}",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("FFmpeg extract_full_audio error: {}", stderr);
        return Err(AppError::Validation(format!(
            "FFmpeg extract_full_audio failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// Нарезает аудиофайл на фрагменты в соответствии с динамическими таймкодами нарративных сегментов.
pub async fn split_audio_by_segments(
    source_audio_path: &Path,
    segments: &[crate::services::video::segmentation::NarrativeSegment],
    output_dir: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    tracing::info!(
        "🎵 Dynamically slicing audio into {} narrative segment(s)...",
        segments.len()
    );

    let mut tasks = Vec::with_capacity(segments.len());

    for (index, seg) in segments.iter().enumerate() {
        let filename = format!("audio_fragment_{:03}.m4a", seg.segment_id);
        let output_path = output_dir.join(&filename);
        let input_path = source_audio_path.to_path_buf();
        let start_str = format!("{:.3}", seg.start_timestamp);
        let end_str = format!("{:.3}", seg.end_timestamp);
        let seg_id = seg.segment_id;

        let task = tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            let output = Command::new("ffmpeg")
                .stdin(Stdio::null())
                .args(["-nostdin", "-y"])
                .args(["-ss", &start_str, "-to", &end_str])
                .args(["-i"])
                .arg(&input_path)
                .args(["-vn", "-c:a", "aac"])
                .arg(&output_path)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| AppError::Validation(format!("Failed to execute ffmpeg: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "FFmpeg audio slice error for segment #{}: {}",
                    seg_id, stderr
                );
                return Err(AppError::Validation(format!(
                    "FFmpeg audio slice failed for segment #{}",
                    seg_id
                )));
            }

            tracing::info!(
                "✅ Narrative audio segment #{} [{}-{}s] extracted in {:.2}s",
                seg_id,
                start_str,
                end_str,
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
            Ok(Err(err)) => return Err(err),
            Err(join_err) => {
                error!("Audio slicing task panicked: {}", join_err);
                return Err(AppError::Validation(format!(
                    "Audio slicing task failed: {}",
                    join_err
                )));
            }
        }
    }

    indexed_files.sort_by_key(|(idx, _)| *idx);
    Ok(indexed_files.into_iter().map(|(_, p)| p).collect())
}

/// Нарезает и склеивает аудиодорожки для каждого клипа: [Отрезок А: Global Intro] + [Отрезок Б: Story]
/// с использованием FFmpeg filter_complex concat ([0:a][1:a]concat=n=2:v=0:a=1[outa])
/// для устранения любого рассинхрона между вступительным вопросом и телом истории.
pub async fn prepare_clip_audio_tracks(
    source_audio_path: &Path,
    clip_plans: &[crate::services::video::segmentation::ClipPlan],
    output_dir: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    tracing::info!(
        "🎵 Assembling {} clip audio track(s) with seamless intro hook...",
        clip_plans.len()
    );

    let mut tasks = Vec::with_capacity(clip_plans.len());

    for (index, plan) in clip_plans.iter().enumerate() {
        let filename = format!("audio_fragment_{:03}.m4a", plan.clip_index);
        let output_path = output_dir.join(&filename);
        let input_path = source_audio_path.to_path_buf();
        let intro_val = plan
            .intro
            .as_ref()
            .ok_or_else(|| AppError::Validation("Clip plan missing intro".to_string()))?;
        let intro_start = format!("{:.3}", intro_val.start_timestamp);
        let intro_end = format!("{:.3}", intro_val.end_timestamp);
        let story_start = format!("{:.3}", plan.story.start_timestamp);
        let story_end = format!("{:.3}", plan.story.end_timestamp);
        let clip_idx = plan.clip_index;

        let task = tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            let pause_dur = crate::services::video::segmentation::intro_pause_duration();
            let filter_complex = if pause_dur > 0.0 {
                format!(
                    "[0:a]apad=pad_dur={pad:.3}[a0];[a0][1:a]concat=n=2:v=0:a=1[outa]",
                    pad = pause_dur
                )
            } else {
                "[0:a][1:a]concat=n=2:v=0:a=1[outa]".to_string()
            };

            let output = Command::new("ffmpeg")
                .stdin(Stdio::null())
                .args(["-nostdin", "-y"])
                .args(["-ss", &intro_start, "-to", &intro_end, "-i"])
                .arg(&input_path)
                .args(["-ss", &story_start, "-to", &story_end, "-i"])
                .arg(&input_path)
                .args([
                    "-filter_complex",
                    &filter_complex,
                    "-map",
                    "[outa]",
                    "-c:a",
                    "aac",
                ])
                .arg(&output_path)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| {
                    AppError::Validation(format!("Failed to execute ffmpeg concat audio: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "FFmpeg audio concat error for clip #{}: {}",
                    clip_idx, stderr
                );
                return Err(AppError::Validation(format!(
                    "FFmpeg audio concat failed for clip #{}: {}",
                    clip_idx, stderr
                )));
            }

            tracing::info!(
                "✅ Seamless clip audio #{} [intro: {}-{}s + story: {}-{}s] assembled in {:.2}s",
                clip_idx,
                intro_start,
                intro_end,
                story_start,
                story_end,
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
            Ok(Err(err)) => return Err(err),
            Err(join_err) => {
                error!("Audio assembling task panicked: {}", join_err);
                return Err(AppError::Validation(format!(
                    "Audio assembling task panicked: {}",
                    join_err
                )));
            }
        }
    }

    indexed_files.sort_by_key(|(idx, _)| *idx);
    Ok(indexed_files.into_iter().map(|(_, p)| p).collect())
}

/// Нарезает аудиодорожки напрямую по границам историй без конкатенации intro (режим include_intro = false).
pub async fn prepare_story_audio_tracks(
    source_audio_path: &Path,
    clip_plans: &[crate::services::video::segmentation::ClipPlan],
    output_dir: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    tracing::info!(
        "🎵 Slicing {} story audio track(s) without intro hook...",
        clip_plans.len()
    );

    let mut tasks = Vec::with_capacity(clip_plans.len());

    for (index, plan) in clip_plans.iter().enumerate() {
        let filename = format!("audio_fragment_{:03}.m4a", plan.clip_index);
        let output_path = output_dir.join(&filename);
        let input_path = source_audio_path.to_path_buf();
        let story_start = format!("{:.3}", plan.story.start_timestamp);
        let story_end = format!("{:.3}", plan.story.end_timestamp);
        let clip_idx = plan.clip_index;

        let task = tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            let output = Command::new("ffmpeg")
                .stdin(Stdio::null())
                .args(["-nostdin", "-y"])
                .args(["-ss", &story_start, "-to", &story_end, "-i"])
                .arg(&input_path)
                .args(["-vn", "-c:a", "aac"])
                .arg(&output_path)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| {
                    AppError::Validation(format!(
                        "Failed to execute ffmpeg story audio slice: {}",
                        e
                    ))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "FFmpeg story audio slice error for clip #{}: {}",
                    clip_idx, stderr
                );
                return Err(AppError::Validation(format!(
                    "FFmpeg story audio slice failed for clip #{}: {}",
                    clip_idx, stderr
                )));
            }

            tracing::info!(
                "✅ Story audio slice #{} [{}-{}s] completed in {:.2}s",
                clip_idx,
                story_start,
                story_end,
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
            Ok(Err(err)) => return Err(err),
            Err(join_err) => {
                error!("Story audio slicing task panicked: {}", join_err);
                return Err(AppError::Validation(format!(
                    "Story audio slicing task panicked: {}",
                    join_err
                )));
            }
        }
    }

    indexed_files.sort_by_key(|(idx, _)| *idx);
    Ok(indexed_files.into_iter().map(|(_, p)| p).collect())
}

pub async fn extract_and_split_audio(
    input_video_path: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, AppError> {
    tracing::info!("🎵 Analyzing video duration for {:?}", input_video_path);
    let duration = get_video_duration(input_video_path).await?;
    let intervals = calculate_intervals(duration);
    tracing::info!(
        "🎵 Video duration: {:.2}s, splitting into {} audio fragment(s)",
        duration,
        intervals.len()
    );

    let mut tasks = Vec::with_capacity(intervals.len());

    for (index, interval) in intervals.into_iter().enumerate() {
        let filename = format!("audio_fragment_{:03}.m4a", index + 1);
        let output_path = output_dir.join(&filename);
        let input_path = input_video_path.to_path_buf();

        let task = tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            let output = Command::new("ffmpeg")
                .stdin(Stdio::null())
                .args(["-nostdin", "-y", "-i"])
                .arg(&input_path)
                .args([
                    "-ss",
                    &interval.start.to_string(),
                    "-to",
                    &interval.end.to_string(),
                    "-vn",
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
                    "FFmpeg audio extraction error for fragment {}: {}",
                    index + 1,
                    stderr
                );
                return Err(AppError::Validation(format!(
                    "FFmpeg audio extraction failed for fragment {}",
                    index + 1
                )));
            }

            tracing::info!(
                "✅ Audio fragment {} extracted in {:.2}s",
                index + 1,
                start_time.elapsed().as_secs_f32()
            );

            Ok((index, output_path))
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;
    let mut generated_files = Vec::with_capacity(results.len());

    for result in results {
        match result {
            Ok(Ok((_index, path))) => {
                generated_files.push(path);
            }
            Ok(Err(err)) => {
                return Err(err);
            }
            Err(join_err) => {
                error!("Audio extraction task panicked: {}", join_err);
                return Err(AppError::Validation(format!(
                    "Audio extraction task failed: {}",
                    join_err
                )));
            }
        }
    }

    // Immediately delete the original video file after successful audio extraction
    if let Err(e) = fs::remove_file(input_video_path).await {
        error!(
            "Failed to remove original video file after audio extraction: {}",
            e
        );
        return Err(AppError::Validation(format!(
            "Failed to delete original video file: {}",
            e
        )));
    }

    tracing::info!("🎵 Audio extraction complete. Cleaned up original video file.");

    Ok(generated_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_intervals_short() {
        let intervals = calculate_intervals(45.0);
        assert_eq!(intervals.len(), 1);
        assert_eq!(
            intervals[0],
            Interval {
                start: 0.0,
                end: 45.0
            }
        );
    }

    #[test]
    fn test_calculate_intervals_exact_60() {
        let intervals = calculate_intervals(60.0);
        assert_eq!(intervals.len(), 1);
        assert_eq!(
            intervals[0],
            Interval {
                start: 0.0,
                end: 60.0
            }
        );
    }

    #[test]
    fn test_calculate_intervals_5_min() {
        let intervals = calculate_intervals(300.0);
        assert_eq!(intervals.len(), 5);
        assert_eq!(
            intervals[0],
            Interval {
                start: 0.0,
                end: 60.0
            }
        );
        assert_eq!(
            intervals[1],
            Interval {
                start: 58.0,
                end: 120.0
            }
        );
        assert_eq!(
            intervals[2],
            Interval {
                start: 118.0,
                end: 180.0
            }
        );
        assert_eq!(
            intervals[3],
            Interval {
                start: 178.0,
                end: 240.0
            }
        );
        assert_eq!(
            intervals[4],
            Interval {
                start: 238.0,
                end: 300.0
            }
        );
    }

    #[test]
    fn test_calculate_intervals_5_min_5_sec() {
        // 5:05 -> 305s.
        // Tail is 7 seconds, must be discarded.
        let intervals = calculate_intervals(305.0);
        assert_eq!(intervals.len(), 5);
        assert_eq!(
            intervals[4],
            Interval {
                start: 238.0,
                end: 300.0
            }
        );
    }

    #[test]
    fn test_calculate_intervals_5_min_10_sec() {
        // 5:10 -> 310s.
        // Tail is 12 seconds, must be included.
        let intervals = calculate_intervals(310.0);
        assert_eq!(intervals.len(), 6);
        assert_eq!(
            intervals[4],
            Interval {
                start: 238.0,
                end: 300.0
            }
        );
        assert_eq!(
            intervals[5],
            Interval {
                start: 298.0,
                end: 310.0
            }
        );
    }
}
