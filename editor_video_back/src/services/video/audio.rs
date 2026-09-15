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
                    "copy",
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

    tracing::info!(
        "🎵 Audio extraction complete. Cleaned up original video file."
    );

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
