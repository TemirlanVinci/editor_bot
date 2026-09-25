use crate::error::AppError;
use crate::models::tiktok::{
    AccountDto, ClaimDueTaskResponse, CreateAccountRequest, ScheduleClipsRequest,
    ScheduleClipsResponse, UpdateTaskStatusRequest,
};
use chrono::{NaiveDateTime, NaiveTime, Utc};

use sqlx::{PgPool, Row};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

fn get_media_dir() -> PathBuf {
    let media_var = env::var("MEDIA_DIR").unwrap_or_else(|_| "/app/media".to_string());
    PathBuf::from(media_var)
}

fn parse_publish_time(time_str: &str) -> NaiveTime {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() >= 2 {
        let hour: u32 = parts[0].parse().unwrap_or(13);
        let min: u32 = parts[1].parse().unwrap_or(0);
        let sec: u32 = if parts.len() > 2 {
            parts[2].parse().unwrap_or(0)
        } else {
            0
        };
        NaiveTime::from_hms_opt(hour, min, sec)
            .unwrap_or_else(|| NaiveTime::from_hms_opt(13, 0, 0).unwrap())
    } else {
        NaiveTime::from_hms_opt(13, 0, 0).unwrap()
    }
}

pub async fn get_active_accounts(pool: &PgPool) -> Result<Vec<AccountDto>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, cookies_path, proxy_url, publish_time::text, interval_days, is_active
        FROM accounts
        WHERE is_active = TRUE
        ORDER BY id ASC;
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(AccountDto {
            id: row.get("id"),
            name: row.get("name"),
            cookies_path: row.get("cookies_path"),
            proxy_url: row.get("proxy_url"),
            publish_time: row
                .get::<Option<String>, _>("publish_time")
                .unwrap_or_else(|| "13:00:00".to_string()),
            interval_days: row.get::<Option<i32>, _>("interval_days").unwrap_or(1),
            is_active: row.get("is_active"),
        });
    }

    Ok(accounts)
}

pub async fn get_account_by_id(pool: &PgPool, id: i32) -> Result<Option<AccountDto>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, cookies_path, proxy_url, publish_time::text, interval_days, is_active
        FROM accounts
        WHERE id = $1;
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        Ok(Some(AccountDto {
            id: r.get("id"),
            name: r.get("name"),
            cookies_path: r.get("cookies_path"),
            proxy_url: r.get("proxy_url"),
            publish_time: r
                .get::<Option<String>, _>("publish_time")
                .unwrap_or_else(|| "13:00:00".to_string()),
            interval_days: r.get::<Option<i32>, _>("interval_days").unwrap_or(1),
            is_active: r.get("is_active"),
        }))
    } else {
        Ok(None)
    }
}

pub async fn create_account(
    pool: &PgPool,
    req: &CreateAccountRequest,
) -> Result<AccountDto, AppError> {
    let p_time = req.publish_time.as_deref().unwrap_or("13:00:00");
    let interval = req.interval_days.unwrap_or(1);
    let active = req.is_active.unwrap_or(true);
    let proxy = req.proxy_url.as_deref().unwrap_or("");

    let time_obj = parse_publish_time(p_time);

    let row = sqlx::query(
        r#"
        INSERT INTO accounts (name, cookies_path, proxy_url, publish_time, interval_days, is_active)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, name, cookies_path, proxy_url, publish_time::text, interval_days, is_active;
        "#,
    )
    .bind(&req.name)
    .bind(&req.cookies_path)
    .bind(proxy)
    .bind(time_obj)
    .bind(interval)
    .bind(active)
    .fetch_one(pool)
    .await?;

    Ok(AccountDto {
        id: row.get("id"),
        name: row.get("name"),
        cookies_path: row.get("cookies_path"),
        proxy_url: row.get("proxy_url"),
        publish_time: row
            .get::<Option<String>, _>("publish_time")
            .unwrap_or_else(|| "13:00:00".to_string()),
        interval_days: row.get::<Option<i32>, _>("interval_days").unwrap_or(1),
        is_active: row.get("is_active"),
    })
}

pub async fn delete_account(pool: &PgPool, id: i32) -> Result<bool, AppError> {
    let res = sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(res.rows_affected() > 0)
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

pub async fn schedule_clips(
    pool: &PgPool,
    req: &ScheduleClipsRequest,
) -> Result<ScheduleClipsResponse, AppError> {
    let account = get_account_by_id(pool, req.account_id)
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

    let source_dir = if clips_dir.exists() {
        clips_dir
    } else if tmp_job_dir.exists() {
        tmp_job_dir.clone()
    } else {
        let fallback_tmp = Path::new("/tmp").join(format!("job_{}", req.job_id));
        let fallback_clips = fallback_tmp.join("clips");
        if fallback_clips.exists() {
            fallback_clips
        } else if fallback_tmp.exists() {
            fallback_tmp
        } else {
            return Err(AppError::Validation(format!(
                "Job directory for job_id {} not found on server",
                req.job_id
            )));
        }
    };

    let mut clip_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&source_dir) {
        for entry in entries.flatten() {
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

    let max_scheduled: Option<NaiveDateTime> = sqlx::query_scalar(
        "SELECT MAX(scheduled_at) FROM queue WHERE account_id = $1 AND status = 'pending';",
    )
    .bind(req.account_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let p_time = parse_publish_time(&account.publish_time);
    let interval_days = account.interval_days.max(1) as i64;
    let now = Utc::now().naive_utc();

    let start_date: NaiveDateTime = match max_scheduled {
        Some(latest) => {
            let next_day = latest.date() + chrono::Duration::days(interval_days);
            NaiveDateTime::new(next_day, p_time)
        }
        None => {
            let target_today = NaiveDateTime::new(now.date(), p_time);
            if target_today <= now {
                NaiveDateTime::new(now.date() + chrono::Duration::days(1), p_time)
            } else {
                target_today
            }
        }
    };

    let acc_storage_dir = media_dir.join(format!("acc_{}", req.account_id));
    fs::create_dir_all(&acc_storage_dir).map_err(|e| {
        AppError::Internal(format!(
            "Failed to create storage directory {:?}: {}",
            acc_storage_dir, e
        ))
    })?;

    let mut tx = pool.begin().await?;

    for (idx, (_filename, src_path)) in clip_files.iter().enumerate() {
        let scheduled_at = start_date + chrono::Duration::days(idx as i64 * interval_days);
        let dest_filename = format!("job_{}_part_{:02}.mp4", req.job_id, idx + 1);
        let dest_path = acc_storage_dir.join(&dest_filename);

        fs::rename(src_path, &dest_path)
            .or_else(|_| fs::copy(src_path, &dest_path).map(|_| ()))
            .map_err(|e| {
                AppError::Internal(format!("Failed to move file to {:?}: {}", dest_path, e))
            })?;

        let random_tags = super::hashtags::get_random_hashtags(pool, 5)
            .await
            .unwrap_or_default();
        let tags_str = if random_tags.is_empty() {
            "#fyp #viral".to_string()
        } else {
            random_tags.join(" ")
        };
        let caption = format!("Part {}/{} | {}", idx + 1, total_clips, tags_str);
        let dest_path_str = dest_path.to_string_lossy().to_string();

        sqlx::query(
            r#"
            INSERT INTO queue (account_id, file_path, caption, scheduled_at, status)
            VALUES ($1, $2, $3, $4, 'pending');
            "#,
        )
        .bind(req.account_id)
        .bind(&dest_path_str)
        .bind(&caption)
        .bind(scheduled_at)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Cleanup parent temp folder
    let parent_tmp = source_dir.parent().unwrap_or(&source_dir);
    let _ = fs::remove_dir_all(parent_tmp);
    let _ = fs::remove_dir_all(&source_dir);

    info!(
        "Scheduled {} clips for account #{} starting at {}",
        total_clips, req.account_id, start_date
    );

    Ok(ScheduleClipsResponse {
        scheduled_count: total_clips,
        first_scheduled_at: start_date.format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

pub async fn claim_due_task(pool: &PgPool) -> Result<Option<ClaimDueTaskResponse>, AppError> {
    let mut tx = pool.begin().await?;
    let now = Utc::now().naive_utc();

    let row = sqlx::query(
        r#"
        SELECT q.id, q.account_id, q.file_path, q.caption, q.scheduled_at::text,
               a.proxy_url, a.cookies_path
        FROM queue q
        JOIN accounts a ON q.account_id = a.id
        WHERE q.status = 'pending' AND q.scheduled_at <= $1
        ORDER BY q.scheduled_at ASC
        FOR UPDATE OF q SKIP LOCKED
        LIMIT 1;
        "#,
    )
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(r) = row {
        let task_id: i32 = r.get("id");

        sqlx::query("UPDATE queue SET status = 'uploading' WHERE id = $1;")
            .bind(task_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(Some(ClaimDueTaskResponse {
            id: task_id,
            account_id: r.get("account_id"),
            file_path: r.get("file_path"),
            caption: r.get("caption"),
            scheduled_at: r
                .get::<Option<String>, _>("scheduled_at")
                .unwrap_or_default(),
            proxy_url: r.get("proxy_url"),
            cookies_path: r.get("cookies_path"),
        }))
    } else {
        tx.commit().await?;
        Ok(None)
    }
}

pub async fn update_task_status(
    pool: &PgPool,
    req: &UpdateTaskStatusRequest,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE queue
        SET status = $1, error_log = $2
        WHERE id = $3;
        "#,
    )
    .bind(&req.status)
    .bind(&req.error_log)
    .bind(req.task_id)
    .execute(pool)
    .await?;

    if req.status == "published"
        && let Ok(Some(file_path)) =
            sqlx::query_scalar::<_, String>("SELECT file_path FROM queue WHERE id = $1")
                .bind(req.task_id)
                .fetch_optional(pool)
                .await
    {
        let path = Path::new(&file_path);
        if path.exists() {
            if let Err(e) = fs::remove_file(path) {
                warn!("Failed to delete published file {}: {}", file_path, e);
            } else {
                info!("Successfully deleted published clip: {}", file_path);
            }
        }
    }

    Ok(())
}
