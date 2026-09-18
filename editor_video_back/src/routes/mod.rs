use axum::{Router, extract::DefaultBodyLimit, routing::post};
use sqlx::PgPool;

pub fn build_router() -> Router<PgPool> {
    Router::new()
        .route("/video/cut", post(crate::handlers::video::cut_video))
        .route(
            "/video/download",
            post(crate::handlers::video::download_video),
        )
        .layer(DefaultBodyLimit::disable())
}
