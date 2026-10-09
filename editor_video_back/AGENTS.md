# Instructions for AI Assistant (editor_video_back)

## 1. Project Context & Architecture

**Purpose:** High-performance REST API backend for automated video cutting, narrative segmentation (Whisper + Qwen LLM), FFmpeg rendering, and TikTok publication queue. Designed to serve the Telegram bot frontend (`editor_video_bot`).

**Project Structure Strategy (Layered Approach):**

- `src/handlers/` — Axum route handlers grouped by domain (`video.rs`, `tiktok.rs`, `hashtags.rs`). Only handles HTTP parsing, validation, calling DB/service functions, and returning responses.
- `src/models/` — Structs, DTOs, Request/Response body shapes (`account.rs`, `queue.rs`, `video.rs`).
- `src/db/` — **Database Layer**. Contains all database queries (`accounts.rs`, `queue.rs`, `hashtags.rs`). Handlers must NOT contain raw SQL queries; they must call functions from this layer.
- `src/routes/` — Route definitions & Axum router configuration.
- `src/error.rs` — Centralized `AppError` type and its `IntoResponse` implementation.
- `src/auth.rs` — Shared-secret auth middleware for bot-to-backend requests (`X-Bot-Secret`).
- `src/services/` — Business logic and external processing layer:
  - `services/video/` — Video downloading (yt-dlp), audio transcription (Whisper), narrative segmentation (Qwen LLM), subtitle rendering (ASS), and clip cutting (FFmpeg).
  - `services/queue.rs` — Clip distribution, humanized publication scheduling, and file management.
- `migrations/` — SQLx database migration scripts.
- `src/main.rs` — Application entry point, DB pool connection, migrations run, background cleanup, and server startup.

---

## 2. Tech Stack & Dependencies

- **Language:** Rust (Edition 2024)
- **Web Framework:** Axum 0.8
- **Async Runtime:** Tokio
- **Database:** PostgreSQL with SQLx (async queries)
- **Speech Recognition:** Whisper (GGML C/C++ bindings via whisper-rs)
- **Narrative Segmentation:** Ollama API (`qwen2.5:3b` / `qwen2.5:7b`)
- **Video & Audio Processing:** FFmpeg & yt-dlp
- **Logging:** `tracing` & `tracing-subscriber` (with `EnvFilter`)
- **Environment:** `dotenvy`
- **Validation:** `validator` (derive-based validation on incoming DTOs)
- **Linting/formatting:** `clippy` + `rustfmt`, enforced as part of the definition of "done"

---

## 3. Mandatory Coding Standards & Best Practices

### Error Handling & Logging

- **NO `unwrap()` or `expect()` in handler or core processing logic.** The only acceptable place for `expect()` is `main.rs` during initial startup.
- **Use centralized `AppError` enum** defined in `src/error.rs`:
  - `NotFound` -> 404
  - `Validation(String)` -> 400
  - `Unauthorized` -> 401
  - `Internal(String)` / `Database(sqlx::Error)` -> 500 (clean error response to client, detailed log via `tracing::error!`).
- **Never leak internal DB/system stacktraces to client.** Log details internally, return clean error message.

### Database Patterns (SQLx)

- Handlers access the database through `State(pool): State<PgPool>`.
- Operations touching multiple rows or updating task states must run inside `pool.begin()` / `Transaction`.
- Avoid unguarded writes; use `SKIP LOCKED` for queue claim operations to support safe concurrent worker polling.

### Authentication

- Every route under `/api/v1` is protected by `X-Bot-Secret` checked via middleware.
- Only `/health` is exposed without authentication.

### Code Quality Gates

- `cargo fmt --check` must pass with no diff.
- `cargo clippy --all-targets -- -D warnings` must pass with zero warnings before code is considered done.
- `cargo test` must pass all unit and integration tests.

---

## 4. Useful Terminal Commands

- `cargo check` — Fast compilation check
- `cargo run` — Run the development server
- `cargo fmt --check` — Verify formatting
- `cargo clippy --all-targets -- -D warnings` — Lint, fail on any warning
- `cargo test` — Run all unit and integration tests
- `sqlx migrate run` — Execute pending SQL migrations
