use axum::Router;
use sqlx::PgPool;

pub mod auth;
pub mod db;
pub mod error;
pub mod handlers;
pub mod models;
pub mod routes;

pub use error::AppError;

/// Builds the main Axum Router with protected API routes under `/api/v1` and health check,
/// injected with the PostgreSQL connection pool.
pub fn create_app(pool: PgPool) -> Router {
    let api_router =
        routes::build_router().route_layer(axum::middleware::from_fn(auth::auth_middleware));

    Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
        .merge(api_router)
        .with_state(pool)
}
