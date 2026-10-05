use crate::db::accounts as db_accounts;
use crate::db::hashtags as db_hashtags;
use crate::db::queue as db_queue;
use crate::error::AppError;
use crate::models::queue::{
    ClearAccountVideosResponse, ScheduleClipsRequest, ScheduleClipsResponse,
    UpdateTaskStatusRequest,
};
use chrono::{NaiveDateTime, NaiveTime, Utc};
use rand::Rng;
use sqlx::PgPool;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

fn get_media_dir() -> PathBuf {
    let media_var = env::var("MEDIA_DIR").unwrap_or_else(|_| "/app/media".to_string());
    PathBuf::from(media_var)
}

fn extract_file_number(filename: &str) -> usize {
    let mut num_str = String::new();
    for c in filename.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
        } else if !num_str.is_empty() {
            break;
        }
    }
    num_str.parse::<usize>().unwrap_or(0)
}

fn get_next_base_slot(after_dt: NaiveDateTime, times: &[NaiveTime]) -> NaiveDateTime {
    let default_slot = NaiveTime::from_hms_opt(13, 0, 0).expect("Default time is valid");
    let sorted_times = if times.is_empty() {
        vec![default_slot]
    } else {
        let mut t = times.to_vec();
        t.sort();
        t
    };

    let current_date = after_dt.date();
    let current_time = after_dt.time();

    for &t in &sorted_times {
        if t > current_time {
            return NaiveDateTime::new(current_date, t);
        }
    }

    let next_date = current_date + chrono::Duration::days(1);
    NaiveDateTime::new(next_date, sorted_times[0])
}

fn apply_human_jitter(
    base_slot: NaiveDateTime,
    now: NaiveDateTime,
    prev_scheduled: Option<NaiveDateTime>,
) -> NaiveDateTime {
    let mut rng = rand::thread_rng();
    // Human-like jitter:
    // - Random minute offset: -12 to +18 minutes around target slot
    // - Random second offset: 7 to 53 seconds (never round :00 seconds)
    let jitter_min: i64 = rng.gen_range(-12..=18);
    let jitter_sec: i64 = rng.gen_range(7..=53);
    let mut scheduled_at = base_slot + chrono::Duration::minutes(jitter_min) + chrono::Duration::seconds(jitter_sec);

    // Safety 1: Must always be strictly in the future (> now)
    if scheduled_at <= now {
        let delay_min: i64 = rng.gen_range(8..=25);
        scheduled_at = now + chrono::Duration::minutes(delay_min) + chrono::Duration::seconds(jitter_sec);
    }

    // Safety 2: Must be at least 45 minutes after the previous scheduled item to avoid stacking
    if let Some(prev) = prev_scheduled {
        let min_gap = chrono::Duration::minutes(45);
        if scheduled_at < prev + min_gap {
            scheduled_at = prev + min_gap + chrono::Duration::minutes(rng.gen_range(5..=20)) + chrono::Duration::seconds(jitter_sec);
        }
    }

    scheduled_at
}

pub async fn schedule_clips(
    pool: &PgPool,
    req: &ScheduleClipsRequest,
) -> Result<ScheduleClipsResponse, AppError> {
    let account = db_accounts::get_account_by_id(pool, req.account_id)
        .await?
        .ok_or_else(|| AppError::Validation(format!("Account #{} not found", req.account_id)))?;

    if !account.is_active {
        return Err(AppError::Validation(format!(
            "Account #{} is inactive",
            req.account_id
        )));
    }

    let media_dir = get_media_dir();
    let tmp_job_dir = media_dir.join("tmp").join(format!("job_{}", req.job_id));
    let clips_dir = tmp_job_dir.join("clips");

    let source_dir = if tokio::fs::try_exists(&clips_dir).await.unwrap_or(false) {
        clips_dir
    } else if tokio::fs::try_exists(&tmp_job_dir).await.unwrap_or(false) {
        tmp_job_dir.clone()
    } else {
        let fallback_tmp = Path::new("/tmp").join(format!("job_{}", req.job_id));
        let fallback_clips = fallback_tmp.join("clips");
        if tokio::fs::try_exists(&fallback_clips)
            .await
            .unwrap_or(false)
        {
            fallback_clips
        } else if tokio::fs::try_exists(&fallback_tmp).await.unwrap_or(false) {
            fallback_tmp
        } else {
            return Err(AppError::Validation(format!(
                "Job directory for job_id {} not found on server",
                req.job_id
            )));
        }
    };

    let mut clip_files = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&source_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
                && matches!(ext.to_lowercase().as_str(), "mp4" | "mov" | "mkv")
                && let Some(filename) = path.file_name().and_then(|f| f.to_str())
            {
                clip_files.push((filename.to_string(), path));
            }
        }
    }

    clip_files.sort_by_key(|(name, _)| extract_file_number(name));

    if clip_files.is_empty() {
        return Err(AppError::Validation(format!(
            "No clip files found in job directory for job_id {}",
            req.job_id
        )));
    }

    let total_clips = clip_files.len();

    let max_scheduled = db_queue::get_max_scheduled_time(pool, req.account_id).await?;
    let times = db_accounts::parse_publish_times(&account.publish_time);
    let now = Utc::now().naive_utc();

    let mut current_base = match max_scheduled {
        Some(latest) if latest > now => latest,
        _ => now,
    };
    let mut prev_scheduled: Option<NaiveDateTime> = max_scheduled;

    let acc_storage_dir = media_dir.join(format!("acc_{}", req.account_id));
    tokio::fs::create_dir_all(&acc_storage_dir)
        .await
        .map_err(|e| {
            AppError::Internal(format!(
                "Failed to create storage directory {:?}: {}",
                acc_storage_dir, e
            ))
        })?;

    // Pre-fetch hashtags once for all clips instead of querying N times in loop
    let random_tags = db_hashtags::get_random_hashtags(pool, (total_clips * 5) as i64)
        .await
        .unwrap_or_default();

    // Проверяем наличие segments.json для сохранения нарративных метаданных
    let segments_file = source_dir.join("segments.json");
    let parent_segments_file = source_dir.parent().map(|p| p.join("segments.json"));
    let segments_data = if tokio::fs::try_exists(&segments_file).await.unwrap_or(false) {
        tokio::fs::read_to_string(&segments_file).await.ok()
    } else if let Some(parent_file) = parent_segments_file
        && tokio::fs::try_exists(&parent_file).await.unwrap_or(false)
    {
        tokio::fs::read_to_string(&parent_file).await.ok()
    } else {
        None
    };

    let segmentation: Option<crate::services::video::segmentation::SegmentationResult> =
        segments_data.and_then(|json_str| serde_json::from_str(&json_str).ok());

    let mut tx = pool.begin().await?;
    let mut first_scheduled_at = None;

    for (idx, (_filename, src_path)) in clip_files.iter().enumerate() {
        let base_slot = get_next_base_slot(current_base, &times);
        current_base = base_slot;

        let scheduled_at = apply_human_jitter(base_slot, now, prev_scheduled);
        prev_scheduled = Some(scheduled_at);
        if first_scheduled_at.is_none() {
            first_scheduled_at = Some(scheduled_at);
        }

        let dest_filename = format!("job_{}_part_{:02}.mp4", req.job_id, idx + 1);
        let dest_path = acc_storage_dir.join(&dest_filename);

        if let Err(_e) = tokio::fs::rename(src_path, &dest_path).await {
            tokio::fs::copy(src_path, &dest_path).await.map_err(|e| {
                AppError::Internal(format!("Failed to move file to {:?}: {}", dest_path, e))
            })?;
        }

        let chunk_start = (idx * 5) % random_tags.len().max(1);
        let chunk_end = (chunk_start + 5).min(random_tags.len());
        let clip_tags = if chunk_start < random_tags.len() {
            &random_tags[chunk_start..chunk_end]
        } else {
            &[]
        };

        let tags_str = if clip_tags.is_empty() {
            "#fyp #viral".to_string()
        } else {
            clip_tags.join(" ")
        };

        let seg_meta = segmentation.as_ref().and_then(|s| s.segments.get(idx));

        let meta = seg_meta.map(|s| db_queue::QueueItemMetadata {
            segment_id: Some(s.segment_id as i32),
            segment_type: Some(s.segment_type.as_str()),
            start_time: Some(s.start_timestamp),
            end_time: Some(s.end_timestamp),
            title: Some(s.title.as_str()),
        });

        let caption = if let Some(s) = seg_meta {
            if s.segment_type == "hook" {
                format!("Hook: {} | {}", s.title, tags_str)
            } else {
                format!(
                    "Part {}/{} - {} | {}",
                    idx + 1,
                    total_clips,
                    s.title,
                    tags_str
                )
            }
        } else {
            format!("Part {}/{} | {}", idx + 1, total_clips, tags_str)
        };

        let dest_path_str = dest_path.to_string_lossy().to_string();

        db_queue::insert_queue_item(
            &mut tx,
            req.account_id,
            &dest_path_str,
            &caption,
            scheduled_at,
            meta.as_ref(),
        )
        .await?;
    }

    tx.commit().await?;

    // Async cleanup of parent temp folder
    let parent_tmp = source_dir.parent().unwrap_or(&source_dir);
    let _ = tokio::fs::remove_dir_all(parent_tmp).await;
    let _ = tokio::fs::remove_dir_all(&source_dir).await;

    let start_date_str = first_scheduled_at
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "N/A".to_string());

    info!(
        account_id = %req.account_id,
        scheduled_count = %total_clips,
        start_date = %start_date_str,
        "Scheduled clips for account"
    );

    Ok(ScheduleClipsResponse {
        scheduled_count: total_clips,
        first_scheduled_at: start_date_str,
    })
}

pub async fn update_task_status(
    pool: &PgPool,
    req: &UpdateTaskStatusRequest,
) -> Result<(), AppError> {
    if let Some(file_path) = db_queue::update_task_status(pool, req).await? {
        let path = Path::new(&file_path);
        if tokio::fs::try_exists(path).await.unwrap_or(false) {
            if let Err(e) = tokio::fs::remove_file(path).await {
                warn!(file_path = %file_path, error = %e, "Failed to delete published file");
            } else {
                info!(file_path = %file_path, "Successfully deleted published clip");
            }
        }
    }

    Ok(())
}

pub async fn clear_account_videos(
    pool: &PgPool,
    account_id: i32,
) -> Result<ClearAccountVideosResponse, AppError> {
    let account = db_accounts::get_account_by_id(pool, account_id)
        .await?
        .ok_or_else(|| AppError::Validation(format!("Account #{account_id} not found")))?;

    let deleted_paths = db_queue::clear_account_queue(pool, account_id).await?;

    let mut deleted_count = 0;
    for file_path in &deleted_paths {
        let path = Path::new(file_path);
        if tokio::fs::try_exists(path).await.unwrap_or(false) {
            if let Err(e) = tokio::fs::remove_file(path).await {
                warn!(file_path = %file_path, error = %e, "Failed to remove video file during archive cleanup");
            } else {
                deleted_count += 1;
            }
        }
    }

    let media_dir = get_media_dir();
    let acc_storage_dir = media_dir.join(format!("acc_{account_id}"));
    if tokio::fs::try_exists(&acc_storage_dir)
        .await
        .unwrap_or(false)
        && let Ok(mut entries) = tokio::fs::read_dir(&acc_storage_dir).await
    {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file()
                && let Err(e) = tokio::fs::remove_file(&path).await
            {
                warn!(path = ?path, error = %e, "Failed to remove file from account dir");
            }
        }
    }

    let final_count = deleted_paths.len().max(deleted_count);

    info!(
        account_id = %account_id,
        account_name = %account.name,
        deleted_count = %final_count,
        "Cleared all video archive files and queue for account"
    );

    Ok(ClearAccountVideosResponse {
        deleted_count: final_count,
        account_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime, Timelike};

    #[test]
    fn test_get_next_base_slot_multi_slot() {
        let times = vec![
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        ];

        let d = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let dt1 = d.and_hms_opt(8, 0, 0).unwrap();
        let next1 = get_next_base_slot(dt1, &times);
        assert_eq!(next1, d.and_hms_opt(10, 0, 0).unwrap());

        let next2 = get_next_base_slot(next1, &times);
        assert_eq!(next2, d.and_hms_opt(18, 0, 0).unwrap());

        let next3 = get_next_base_slot(next2, &times);
        let next_d = NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
        assert_eq!(next3, next_d.and_hms_opt(10, 0, 0).unwrap());
    }

    #[test]
    fn test_apply_human_jitter_properties() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let base = d.and_hms_opt(13, 0, 0).unwrap();
        let now = d.and_hms_opt(12, 0, 0).unwrap();

        let jittered = apply_human_jitter(base, now, None);
        assert!(jittered > now, "Jittered time must be in the future");
        // Non-round seconds
        let sec = jittered.time().second();
        assert!(sec >= 5 && sec <= 55, "Seconds should be non-round: got {}", sec);

        // Test with past now (e.g. now was already 13:05)
        let now_late = d.and_hms_opt(13, 5, 0).unwrap();
        let jittered_late = apply_human_jitter(base, now_late, None);
        assert!(jittered_late > now_late, "Must be pushed ahead of now when late");

        // Test minimum gap with previous item
        let prev = d.and_hms_opt(13, 10, 0).unwrap();
        let close_base = d.and_hms_opt(13, 15, 0).unwrap();
        let spaced = apply_human_jitter(close_base, now, Some(prev));
        assert!(
            spaced >= prev + chrono::Duration::minutes(45),
            "Must maintain minimum gap from previous item"
        );
    }
}
