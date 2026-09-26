use crate::db::accounts as db_accounts;
use crate::db::queue as db_queue;
use crate::error::AppError;
use crate::models::account::{AccountDto, CreateAccountRequest};
use crate::models::queue::{
    ClearAccountVideosResponse, ScheduleClipsRequest, ScheduleClipsResponse,
    UpdateTaskStatusRequest,
};
use crate::services::queue as queue_service;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use validator::Validate;

pub async fn get_accounts(State(pool): State<PgPool>) -> Result<Json<Vec<AccountDto>>, AppError> {
    let accounts = db_accounts::get_active_accounts(&pool).await?;
    Ok(Json(accounts))
}

pub async fn get_account_by_id(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> Result<Json<AccountDto>, AppError> {
    let account = db_accounts::get_account_by_id(&pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(account))
}

pub async fn create_account(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountDto>), AppError> {
    payload.validate()?;
    let account = db_accounts::create_account(&pool, &payload).await?;
    Ok((StatusCode::CREATED, Json(account)))
}

pub async fn delete_account(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let deleted = db_accounts::delete_account(&pool, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

pub async fn clear_account_videos(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> Result<Json<ClearAccountVideosResponse>, AppError> {
    let res = queue_service::clear_account_videos(&pool, id).await?;
    Ok(Json(res))
}

pub async fn schedule_clips(
    State(pool): State<PgPool>,
    Json(payload): Json<ScheduleClipsRequest>,
) -> Result<Json<ScheduleClipsResponse>, AppError> {
    payload.validate()?;
    let res = queue_service::schedule_clips(&pool, &payload).await?;
    Ok(Json(res))
}

pub async fn claim_due_task(State(pool): State<PgPool>) -> Result<Response, AppError> {
    if let Some(task) = db_queue::claim_due_task(&pool).await? {
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
    queue_service::update_task_status(&pool, &payload).await?;
    Ok(StatusCode::OK)
}

