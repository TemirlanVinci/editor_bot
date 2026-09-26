use crate::error::AppError;
use sqlx::PgPool;

/// Fetches all background video file paths registered in the database.
pub async fn fetch_all_background_paths(pool: &PgPool) -> Result<Vec<String>, AppError> {
    let paths = sqlx::query_scalar::<_, String>("SELECT file_path FROM background_videos")
        .fetch_all(pool)
        .await?;
    Ok(paths)
}

/// Batch inserts new background video file paths into the database, ignoring existing ones.
pub async fn insert_background_files_batch(
    pool: &PgPool,
    files: &[String],
) -> Result<(), AppError> {
    if files.is_empty() {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO background_videos (file_path)
        SELECT * FROM UNNEST($1::text[])
        ON CONFLICT (file_path) DO NOTHING;
        "#,
    )
    .bind(files)
    .execute(pool)
    .await?;

    Ok(())
}

/// Batch deletes invalid/missing background video file paths from the database.
pub async fn delete_background_files_batch(
    pool: &PgPool,
    missing_files: &[String],
) -> Result<(), AppError> {
    if missing_files.is_empty() {
        return Ok(());
    }

    sqlx::query("DELETE FROM background_videos WHERE file_path = ANY($1)")
        .bind(missing_files)
        .execute(pool)
        .await?;

    Ok(())
}
