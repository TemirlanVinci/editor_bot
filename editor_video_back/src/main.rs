use axum::http::{Method, header::CONTENT_TYPE};
use dotenvy::dotenv;
// Изменено имя крейта:
use editor_video_back::create_app;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Инициализация переменных окружения
    dotenv().ok();

    // Инициализация логирования — ОДИН РАЗ, в самом начале main
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = env::var("DATABASE_URL").expect("Переменная DATABASE_URL не найдена в .env");

    // Пул соединений с БД
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect(&database_url)
        .await
        .expect("Не удалось подключиться к базе данных");

    info!("✅ Успешное подключение к PostgreSQL!");

    // Запуск миграций с автовосстановлением при VersionMismatch
    if let Err(e) = sqlx::migrate!().run(&pool).await {
        tracing::warn!("⚠️ Ошибка запуска миграций ({}). Выполняем автовосстановление _sqlx_migrations...", e);
        let _ = sqlx::query("DELETE FROM _sqlx_migrations")
            .execute(&pool)
            .await;
        if let Err(retry_err) = sqlx::migrate!().run(&pool).await {
            tracing::warn!("⚠️ Повторный запуск миграций: {}. Продолжаем запуск сервера.", retry_err);
        } else {
            info!("✅ Миграции успешно обновлены!");
        }
    } else {
        info!("✅ Миграции успешно применены!");
    }

    // Настраиваем CORS
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE])
        .allow_origin(Any);

    // Сборка роутера через library helper
    let app = create_app(pool)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Запуск сервера
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);

    // Используем [0, 0, 0, 0] вместо [127, 0, 0, 1] для работы в Docker
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("🚀 Запуск сервера на http://{}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
