use axum::{extract::Request, middleware::Next, response::Response};
use std::env;

use crate::AppError;

/// Middleware for validating requests from the Telegram bot.
/// Checks incoming `X-Bot-Secret` request header against `BOT_SECRET` env variable.
pub async fn auth_middleware(req: Request, next: Next) -> Result<Response, AppError> {
    let expected_secret = env::var("BOT_SECRET").map_err(|_| AppError::Unauthorized)?;

    let provided_secret = req
        .headers()
        .get("X-Bot-Secret")
        .and_then(|val| val.to_str().ok());

    match provided_secret {
        Some(secret) if !secret.is_empty() && secret == expected_secret => Ok(next.run(req).await),
        _ => Err(AppError::Unauthorized),
    }
}
