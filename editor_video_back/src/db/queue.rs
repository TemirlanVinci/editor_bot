use crate::error::AppError;
use crate::models::queue::{ClaimDueTaskResponse, UpdateTaskStatusRequest};
use chrono::{NaiveDateTime, Utc};
use sqlx::{PgPool, Row, Transaction};

pub async fn get_max_scheduled_time(
    pool: &PgPool,
    account_id: i32,
) -> Result<Option<NaiveDateTime>, AppError> {
    let max_scheduled: Option<NaiveDateTime> = sqlx::query_scalar(
        "SELECT MAX(scheduled_at) FROM queue WHERE account_id = $1 AND status = 'pending';",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok(max_scheduled)
}

pub async fn insert_queue_item(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    account_id: i32,
    file_path: &str,
    caption: &str,
    scheduled_at: NaiveDateTime,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO queue (account_id, file_path, caption, scheduled_at, status)
        VALUES ($1, $2, $3, $4, 'pending');
        "#,
    )
    .bind(account_id)
    .bind(file_path)
    .bind(caption)
    .bind(scheduled_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
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
) -> Result<Option<String>, AppError> {
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

    if req.status == "published" {
        let file_path =
            sqlx::query_scalar::<_, String>("SELECT file_path FROM queue WHERE id = $1")
                .bind(req.task_id)
                .fetch_optional(pool)
                .await?;
        Ok(file_path)
    } else {
        Ok(None)
    }
}
