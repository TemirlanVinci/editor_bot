use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct ScheduleClipsRequest {
    pub account_id: i32,
    #[validate(length(min = 1))]
    pub job_id: String,
}

#[derive(Debug, Serialize)]
pub struct ScheduleClipsResponse {
    pub scheduled_count: usize,
    pub first_scheduled_at: String,
}

#[derive(Debug, Serialize)]
pub struct ClaimDueTaskResponse {
    pub id: i32,
    pub account_id: i32,
    pub file_path: String,
    pub caption: String,
    pub scheduled_at: String,
    pub proxy_url: String,
    pub cookies_path: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateTaskStatusRequest {
    pub task_id: i32,
    pub status: String, // 'published', 'failed'
    pub error_log: Option<String>,
}
