use crate::error::AppError;
use sqlx::PgPool;

/// Fetches `count` random hashtags from the hashtags table in PostgreSQL.
pub async fn get_random_hashtags(pool: &PgPool, count: i64) -> Result<Vec<String>, AppError> {
    let limit = count.max(1);
    let rows =
        sqlx::query_scalar::<_, String>("SELECT tag FROM hashtags ORDER BY RANDOM() LIMIT $1;")
            .bind(limit)
            .fetch_all(pool)
            .await?;

    Ok(rows)
}
