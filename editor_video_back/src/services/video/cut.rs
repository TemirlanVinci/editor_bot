use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::fs::File;
use std::io::{Write, Read};
use tokio::process::Command;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;
use crate::error::AppError;
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
        let start = if i == 1 { 0.0 } else { ((i - 1) * 60) as f64 - 2.0 };
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
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input_path)
        .output()
        .await
        .map_err(|e| AppError::Validation(format!("Failed to execute ffprobe: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("ffprobe error: {}", stderr);
        return Err(AppError::Validation("Failed to determine video duration".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let duration: f64 = stdout
        .trim()
        .parse()
        .map_err(|_| AppError::Validation("Failed to parse video duration".to_string()))?;

    Ok(duration)
}

pub async fn split_video(input_path: &Path, output_dir: &Path, duration: f64) -> Result<Vec<PathBuf>, AppError> {
    let intervals = calculate_intervals(duration);
    let mut generated_files = Vec::new();

    for (index, interval) in intervals.into_iter().enumerate() {
        let filename = format!("fragment_{:03}.mp4", index + 1);
        let output_path = output_dir.join(&filename);

        let status = Command::new("ffmpeg")
            .args([
                "-y", // Overwrite
                "-i",
            ])
            .arg(input_path)
            .args([
                "-ss", &interval.start.to_string(),
                "-to", &interval.end.to_string(),
                "-c:v", "copy",
                "-c:a", "copy",
            ])
            .arg(&output_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AppError::Validation(format!("Failed to execute ffmpeg: {}", e)))?;

        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            error!("ffmpeg error on fragment {}: {}", index + 1, stderr);
            return Err(AppError::Validation(format!("FFmpeg failed for fragment {}", index + 1)));
        }

        generated_files.push(output_path);
    }

    Ok(generated_files)
}

pub async fn create_zip(files: Vec<PathBuf>, output_zip_path: PathBuf) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let file = File::create(&output_zip_path)
            .map_err(|e| AppError::Validation(format!("Failed to create zip file: {}", e)))?;
        
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for file_path in files {
            let file_name = file_path.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| AppError::Validation("Invalid file name".to_string()))?;
            
            zip.start_file(file_name, options)
                .map_err(|e| AppError::Validation(format!("Failed to start zip file {}: {}", file_name, e)))?;
            
            let mut f = File::open(&file_path)
                .map_err(|e| AppError::Validation(format!("Failed to open fragment file: {}", e)))?;
            
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)
                .map_err(|e| AppError::Validation(format!("Failed to read fragment file: {}", e)))?;
            
            zip.write_all(&buffer)
                .map_err(|e| AppError::Validation(format!("Failed to write fragment to zip: {}", e)))?;
        }

        zip.finish()
            .map_err(|e| AppError::Validation(format!("Failed to finish zip: {}", e)))?;

        Ok::<(), AppError>(())
    })
    .await
    .map_err(|e| AppError::Validation(format!("Zip task panicked: {}", e)))??;

    Ok(())
}

pub async fn process_video(input_path: &Path, temp_dir: &Path) -> Result<PathBuf, AppError> {
    let duration = get_video_duration(input_path).await?;
    let fragment_paths = split_video(input_path, temp_dir, duration).await?;
    
    let zip_path = temp_dir.join("fragments.zip");
    create_zip(fragment_paths, zip_path.clone()).await?;

    Ok(zip_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_intervals_short() {
        let intervals = calculate_intervals(45.0);
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0], Interval { start: 0.0, end: 45.0 });
    }

    #[test]
    fn test_calculate_intervals_exact_60() {
        let intervals = calculate_intervals(60.0);
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0], Interval { start: 0.0, end: 60.0 });
    }

    #[test]
    fn test_calculate_intervals_5_min() {
        let intervals = calculate_intervals(300.0);
        assert_eq!(intervals.len(), 5);
        assert_eq!(intervals[0], Interval { start: 0.0, end: 60.0 });
        assert_eq!(intervals[1], Interval { start: 58.0, end: 120.0 });
        assert_eq!(intervals[2], Interval { start: 118.0, end: 180.0 });
        assert_eq!(intervals[3], Interval { start: 178.0, end: 240.0 });
        assert_eq!(intervals[4], Interval { start: 238.0, end: 300.0 });
    }

    #[test]
    fn test_calculate_intervals_5_min_5_sec() {
        // 5:05 -> 305s.
        // Tail is 7 seconds, must be discarded.
        let intervals = calculate_intervals(305.0);
        assert_eq!(intervals.len(), 5); // 5 fragments, not 6
        assert_eq!(intervals[4], Interval { start: 238.0, end: 300.0 });
    }

    #[test]
    fn test_calculate_intervals_5_min_10_sec() {
        // 5:10 -> 310s.
        // Tail is 12 seconds, must be included.
        let intervals = calculate_intervals(310.0);
        assert_eq!(intervals.len(), 6);
        assert_eq!(intervals[4], Interval { start: 238.0, end: 300.0 });
        assert_eq!(intervals[5], Interval { start: 298.0, end: 310.0 });
    }
}
