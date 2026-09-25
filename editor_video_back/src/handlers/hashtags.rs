use crate::db::hashtags as db_hashtags;
use crate::error::AppError;
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Deserialize)]
pub struct RandomHashtagsParams {
    pub count: Option<i64>,
}

pub async fn get_random_hashtags(
    State(pool): State<PgPool>,
    Query(params): Query<RandomHashtagsParams>,
) -> Result<Json<Vec<String>>, AppError> {
    let count = params.count.unwrap_or(5);
    let tags = db_hashtags::get_random_hashtags(&pool, count).await?;
    Ok(Json(tags))
}
