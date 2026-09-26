use crate::error::AppError;
use crate::models::account::{AccountDto, CreateAccountRequest};
use chrono::NaiveTime;
use sqlx::{PgPool, Row};

pub fn parse_publish_time(time_str: &str) -> NaiveTime {
    parse_publish_times(time_str)
        .into_iter()
        .next()
        .unwrap_or_else(|| NaiveTime::from_hms_opt(13, 0, 0).expect("Default time is valid"))
}

pub fn parse_publish_times(time_str: &str) -> Vec<NaiveTime> {
    let mut times = Vec::new();
    for part in time_str.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let sub_parts: Vec<&str> = part.split(':').collect();
        if sub_parts.len() >= 2 {
            let hour: u32 = sub_parts[0].parse().unwrap_or(13);
            let min: u32 = sub_parts[1].parse().unwrap_or(0);
            let sec: u32 = if sub_parts.len() > 2 {
                sub_parts[2].parse().unwrap_or(0)
            } else {
                0
            };
            if let Some(t) = NaiveTime::from_hms_opt(hour, min, sec) {
                times.push(t);
            }
        }
    }
    if times.is_empty()
        && let Some(default_t) = NaiveTime::from_hms_opt(13, 0, 0)
    {
        times.push(default_t);
    }
    times.sort();
    times.dedup();
    times
}

pub async fn get_active_accounts(pool: &PgPool) -> Result<Vec<AccountDto>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, cookies_path, proxy_url,
               COALESCE(NULLIF(publish_times, ''), publish_time::text, '13:00') as publish_time,
               interval_days, is_active
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
                .unwrap_or_else(|| "13:00".to_string()),
            interval_days: row.get::<Option<i32>, _>("interval_days").unwrap_or(1),
            is_active: row.get("is_active"),
        });
    }

    Ok(accounts)
}

pub async fn get_account_by_id(pool: &PgPool, id: i32) -> Result<Option<AccountDto>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, cookies_path, proxy_url,
               COALESCE(NULLIF(publish_times, ''), publish_time::text, '13:00') as publish_time,
               interval_days, is_active
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
                .unwrap_or_else(|| "13:00".to_string()),
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
    let p_time = req.publish_time.as_deref().unwrap_or("13:00");
    let interval = req.interval_days.unwrap_or(1);
    let active = req.is_active.unwrap_or(true);
    let proxy = req.proxy_url.as_deref().unwrap_or("");

    let time_obj = parse_publish_time(p_time);

    let row = sqlx::query(
        r#"
        INSERT INTO accounts (name, cookies_path, proxy_url, publish_time, publish_times, interval_days, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, name, cookies_path, proxy_url,
                  COALESCE(NULLIF(publish_times, ''), publish_time::text, '13:00') as publish_time,
                  interval_days, is_active;
        "#,
    )
    .bind(&req.name)
    .bind(&req.cookies_path)
    .bind(proxy)
    .bind(time_obj)
    .bind(p_time)
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
            .unwrap_or_else(|| "13:00".to_string()),
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
