use crate::db::accounts as db_accounts;
use crate::db::hashtags as db_hashtags;
use crate::db::queue as db_queue;
use crate::error::AppError;
use crate::models::queue::{
    ClearAccountVideosResponse, ScheduleClipsRequest, ScheduleClipsResponse,
    UpdateTaskStatusRequest,
};
use chrono::{NaiveDateTime, NaiveTime, Utc};
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

fn get_next_scheduled_time(after_dt: NaiveDateTime, times: &[NaiveTime]) -> NaiveDateTime {
    if times.is_empty() {
        let next_date = after_dt.date() + chrono::Duration::days(1);
        let default_time = NaiveTime::from_hms_opt(13, 0, 0).expect("Default time is valid");
        return NaiveDateTime::new(next_date, default_time);
    }

    let current_date = after_dt.date();
    let current_time = after_dt.time();

    for &t in times {
        if t > current_time {
            return NaiveDateTime::new(current_date, t);
        }
    }

    let next_date = current_date + chrono::Duration::days(1);
    NaiveDateTime::new(next_date, times[0])
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

    let mut current_ref = match max_scheduled {
        Some(latest) if latest > now => latest,
        _ => now,
    };

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

    let mut tx = pool.begin().await?;
    let mut first_scheduled_at = None;

    for (idx, (_filename, src_path)) in clip_files.iter().enumerate() {
        let scheduled_at = get_next_scheduled_time(current_ref, &times);
        current_ref = scheduled_at;
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

        let caption = format!("Part {}/{} | {}", idx + 1, total_clips, tags_str);
        let dest_path_str = dest_path.to_string_lossy().to_string();

        db_queue::insert_queue_item(
            &mut tx,
            req.account_id,
            &dest_path_str,
            &caption,
            scheduled_at,
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
    if tokio::fs::try_exists(&acc_storage_dir).await.unwrap_or(false)
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

