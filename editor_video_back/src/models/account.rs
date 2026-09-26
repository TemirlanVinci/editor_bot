use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountDto {
    pub id: i32,
    pub name: String,
    pub cookies_path: String,
    pub proxy_url: String,
    pub publish_time: String,
    pub interval_days: i32,
    pub is_active: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateAccountRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[validate(length(min = 1, max = 512))]
    pub cookies_path: String,
    pub proxy_url: Option<String>,
    pub publish_time: Option<String>, // HH:MM:SS or HH:MM
    pub interval_days: Option<i32>,
    pub is_active: Option<bool>,
}
