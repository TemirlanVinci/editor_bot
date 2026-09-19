use crate::db::tiktok as db_tiktok;
use crate::error::AppError;
use crate::models::tiktok::{
    AccountDto, CreateAccountRequest, ScheduleClipsRequest, ScheduleClipsResponse,
    UpdateTaskStatusRequest,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use validator::Validate;

pub async fn get_accounts(State(pool): State<PgPool>) -> Result<Json<Vec<AccountDto>>, AppError> {
    let accounts = db_tiktok::get_active_accounts(&pool).await?;
    Ok(Json(accounts))
}

pub async fn get_account_by_id(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> Result<Json<AccountDto>, AppError> {
    let account = db_tiktok::get_account_by_id(&pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(account))
}

pub async fn create_account(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountDto>), AppError> {
    payload.validate()?;
    let account = db_tiktok::create_account(&pool, &payload).await?;
    Ok((StatusCode::CREATED, Json(account)))
}

pub async fn delete_account(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let deleted = db_tiktok::delete_account(&pool, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn schedule_clips(
    State(pool): State<PgPool>,
    Json(payload): Json<ScheduleClipsRequest>,
) -> Result<Json<ScheduleClipsResponse>, AppError> {
    payload.validate()?;
    let res = db_tiktok::schedule_clips(&pool, &payload).await?;
    Ok(Json(res))
}

pub async fn claim_due_task(State(pool): State<PgPool>) -> Result<Response, AppError> {
    if let Some(task) = db_tiktok::claim_due_task(&pool).await? {
        Ok(Json(task).into_response())
    } else {
        Ok(StatusCode::NO_CONTENT.into_response())
    }
}

pub async fn update_task_status(
    State(pool): State<PgPool>,
    Json(payload): Json<UpdateTaskStatusRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;
    db_tiktok::update_task_status(&pool, &payload).await?;
    Ok(StatusCode::OK)
}
