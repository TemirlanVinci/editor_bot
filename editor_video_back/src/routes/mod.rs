use axum::{extract::DefaultBodyLimit, routing::post, Router};
use sqlx::PgPool;

pub fn build_router() -> Router<PgPool> {
    Router::new()
        .route("/video/cut", post(crate::handlers::video::cut_video))
        .layer(DefaultBodyLimit::disable())
}
