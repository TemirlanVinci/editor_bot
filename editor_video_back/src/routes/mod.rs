use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};

use sqlx::PgPool;

pub fn build_router() -> Router<PgPool> {
    Router::new()
        .route("/video/cut", post(crate::handlers::video::cut_video))
        .route(
            "/video/download",
            post(crate::handlers::video::download_video),
        )
        .route(
            "/accounts",
            get(crate::handlers::tiktok::get_accounts)
                .post(crate::handlers::tiktok::create_account),
        )
        .route(
            "/accounts/{id}",
            get(crate::handlers::tiktok::get_account_by_id)
                .delete(crate::handlers::tiktok::delete_account),
        )
        .route(
            "/accounts/{id}/videos",
            delete(crate::handlers::tiktok::clear_account_videos),
        )
        .route(
            "/queue/schedule",
            post(crate::handlers::tiktok::schedule_clips),
        )
        .route(
            "/queue/claim_due",
            post(crate::handlers::tiktok::claim_due_task),
        )
        .route(
            "/queue/update_status",
            post(crate::handlers::tiktok::update_task_status),
        )
        .route(
            "/hashtags/random",
            get(crate::handlers::hashtags::get_random_hashtags),
        )
        .layer(DefaultBodyLimit::disable())
}
