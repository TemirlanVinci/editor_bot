# Instructions for AI Assistant (imperia_pizza_back)

## 1. Project Context & Architecture

**Purpose:** High-performance REST API backend for a Telegram bot (frontend built with `aiogram 3`).

**Project Structure Strategy (Layered Approach):**

- `src/handlers/` — Axum route handlers grouped by domain (`favorites.rs`, `orders.rs`, etc.). Only handles HTTP parsing, validation, calling DB functions, and returning responses.
- `src/models/` — Structs, DTOs, Request/Response body shapes.
- `src/db/` — **Database Layer**. Contains all `sqlx` queries grouped by domain. Handlers must NOT contain raw SQL queries; they must call functions from this layer.
- `src/routes/` — Route definitions & Axum router configurations.
- `src/error.rs` — Centralized `AppError` type and its `IntoResponse` implementation.
- `src/auth.rs` — Shared-secret auth middleware/extractor for bot-to-backend requests.
- `tests/` — **Integration Tests**. Contains API integration tests (e.g., `products_api_test.rs`) using `#[sqlx::test]`.
- `migrations/` — SQLx database migration scripts.
- `.sqlx/` — Offline query cache for `sqlx::query!` macros (committed to git).
- `src/main.rs` — Application entry point & state initialization.

> **Constraint:** Do NOT suggest refactoring to feature-slices right now. Maintain this structure strictly.

---

## 2. Tech Stack & Dependencies

- **Language:** Rust (Edition 2024)
- **Web Framework:** Axum 0.8
- **Async Runtime:** Tokio
- **Database:** PostgreSQL with SQLx 0.9 (async queries)
- **Money type:** `rust_decimal::Decimal` — NEVER `f32`/`f64` for prices, totals, or any monetary value.
- **Logging:** `tracing` & `tracing-subscriber` (with `EnvFilter`)
- **Environment:** `dotenvy`
- **Validation:** `validator` (derive-based validation on incoming DTOs)
- **Linting/formatting:** `clippy` + `rustfmt`, enforced as part of the definition of "done"

---

## 3. Mandatory Coding Standards & Best Practices

### Error Handling & Logging

- **NO `unwrap()` or `expect()` in handler or core logic.** The only acceptable place for `expect()` is `main.rs` during startup (e.g. failing fast if `DATABASE_URL` is missing), and even there prefer a proper error return from `main() -> anyhow::Result<()>`.
- **Use a centralized `AppError` enum**, not raw `(StatusCode, String)` tuples scattered across handlers. Define it once in `src/error.rs`:

  ```rust
  pub enum AppError {
      NotFound,
      Validation(String),
      Database(sqlx::Error),
      Unauthorized,
  }

  impl IntoResponse for AppError {
      fn into_response(self) -> Response {
          let (status, message) = match self {
              AppError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
              AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
              AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
              AppError::Database(e) => {
                  tracing::error!(error = %e, "database error");
                  (StatusCode::INTERNAL_SERVER_ERROR, "internal server error".to_string())
              }
          };
          (status, message).into_response()
      }
  }

  impl From<sqlx::Error> for AppError {
      fn from(e: sqlx::Error) -> Self {
          AppError::Database(e)
      }
  }
  ```

  This lets handlers use `?` instead of manual `.map_err()` chains everywhere, while keeping the "never leak DB internals" rule enforced in exactly one place.

- **Log Errors Contextually:** Always use `tracing::error!` (or `tracing::warn!` for expected/recoverable cases) before returning an error response. Prefer structured fields (`tracing::error!(error = %e, order_id = %id, "failed to fetch order")`) over string interpolation.
- **Keep Client Responses Clean:** Internal SQLx / DB errors must NEVER be leaked to the client. Log the exact `%e` error via `tracing`, return a generic `"internal server error"` message with `StatusCode::INTERNAL_SERVER_ERROR`.

### Money & Numeric Data

- All prices, totals, discounts, and any currency value MUST use `rust_decimal::Decimal` in Rust and `NUMERIC`/`DECIMAL` in Postgres — never `FLOAT`/`f32`/`f64`, and never plain integers pretending to be "cents" unless explicitly documented as such in the column comment.
- Quantity fields (pizza count, etc.) are plain integers (`i32`), not decimals.
- Any arithmetic on money (totals, discounts) happens server-side. Never trust a total sent from the client — always recompute from the DB-stored prices.

### Database Patterns (SQLx)

- **Prefer compile-time checked queries:** Use `sqlx::query!` and `sqlx::query_as!` macros for all SQL operations whenever the query is static.
- **Offline mode is mandatory for CI/builds without DB access:** Run `cargo sqlx prepare` before every commit that touches a query, and commit the resulting `.sqlx/` directory. Never commit code that only compiles with a live `DATABASE_URL`.
- **Avoid manual dynamic binding:** Use macro parameters (`sqlx::query!("...", arg1, arg2)`) instead of `.bind()`.
- **Dynamic queries are the one exception:** if a query must branch on optional filters, use `sqlx::QueryBuilder` rather than hand-built string concatenation, and document why the macro couldn't be used.
- **Transactions for multi-step writes:** Any operation touching more than one table in a single logical action (e.g. creating an order + its order_items) MUST run inside `pool.begin()` / `Transaction`, committed only after all steps succeed. Do not do sequential unguarded inserts for multi-table writes.
- **State Injection:** Always pass `State(pool): State<PgPool>` for DB access in Axum handlers.

### Authentication

- Every route (except an explicit `/health` check) MUST be protected by a shared-secret check between the bot and this backend — e.g. a static bot API key sent in a custom header (`X-Bot-Secret`), validated in Axum middleware before the request reaches any handler.
- The secret lives in `.env` / environment variables, never hardcoded, never logged.
- This is not optional even for an internal/private API — assume the endpoint URL can leak.

### Input Validation

- All `Json<T>` / `Query<T>` request bodies MUST derive `validator::Validate` and be explicitly validated (`payload.validate()`) at the top of the handler, before touching the database. Return `AppError::Validation(...)` on failure.
- Never rely solely on Postgres constraints to catch bad input — validate in the app layer first so the client gets a clean, specific error message instead of a generic 500.

### Handler Structure Pattern

Keep handlers consistent across the codebase:

- **Signatures:** Accept `State(pool): State<PgPool>` and optional `Json<T>` / `Query<T>` payload.
- **Returns:** `Result<Json<T>, AppError>` or `Result<StatusCode, AppError>`.
- **Error Flow:** Use `?` to propagate errors into `AppError` via `From` impls; log happens inside `AppError::into_response`, not scattered in every handler.
- **Validation first, then business logic, then DB call** — in that order, every time.

### Code Quality Gates

- `cargo fmt --check` must pass with no diff.
- `cargo clippy --all-targets -- -D warnings` must pass with zero warnings before code is considered done.
- No `#[allow(...)]` attributes to silence clippy without a one-line comment explaining why.

### Testing

- Handlers that contain non-trivial logic (anything beyond a single straight-line query) should have an integration test using `#[sqlx::test]`, which spins up an isolated test database per test.
- Do not test against the production or dev database.

---

## 4. Useful Terminal Commands

- `cargo check` — Fast compilation check
- `cargo run` — Run the development server
- `cargo fmt --check` — Verify formatting
- `cargo clippy --all-targets -- -D warnings` — Lint, fail on any warning
- `sqlx migrate run` — Execute pending SQL migrations
- `cargo sqlx prepare` — Regenerate the `.sqlx/` offline query cache (run after any query change, before commit)
- `cargo sqlx migrate add <name>` — Create a new migration file
