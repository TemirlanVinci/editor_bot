# Instructions for AI Assistant (editor_video_bot)

## 1. Project Context & Architecture

**Purpose:** Telegram bot frontend/service built with Python and `aiogram 3`.

The bot is responsible for:

- receiving user input and files from Telegram;
- handling Telegram commands and callbacks;
- communicating with the Rust backend over HTTP;
- sending backend results back to Telegram users;
- formatting messages and managing Telegram UI.

The bot must NOT contain video-processing business logic. Video processing is handled by the Rust backend.

### Project Structure

- `bot/handlers/` — Telegram handlers. Contains handlers for commands, messages, callbacks, and user interactions. Handlers should coordinate the bot flow and call appropriate API/service functions rather than contain large amounts of business logic.

- `bot/api/` — HTTP client/API layer for communication with the Rust backend. All backend HTTP requests should be placed here rather than directly inside Telegram handlers.

- `bot/keyboards/` — Telegram inline/reply keyboards and related UI definitions.

- `bot/config.py` — Application configuration and environment variable loading.

- `filters.py` — Custom aiogram filters.

- `formatting.py` — Message formatting and reusable text-building helpers.

- `logger.py` — Logging configuration.

- `main.py` — Application entry point, bot/dispatcher initialization, router registration, and startup/shutdown logic.

- `requirements.txt` — Python dependencies.

- `Dockerfile` — Container configuration for the Telegram bot.

- `venv/` — Local development virtual environment. Never modify, commit, or use it as part of the application source code.

### Architecture Rule

Keep the following dependency direction:

Telegram update
→ handler
→ API/service function
→ Rust backend
→ response
→ handler
→ Telegram response

Handlers should not contain raw HTTP implementation details when a corresponding function can be placed in `bot/api/`.

Do NOT move Rust backend logic into the Python bot.

---

## 2. Tech Stack

- **Language:** Python
- **Telegram framework:** `aiogram 3`
- **HTTP client:** use the HTTP client already present in `requirements.txt`; do not introduce another HTTP library without a reason.
- **Configuration:** environment variables / `.env`
- **Logging:** Python logging through the project's `logger.py`
- **Runtime:** Docker

Do not introduce additional frameworks or libraries unless they are actually necessary for the current task.

---

## 3. General Coding Standards

### Keep Code Simple

Python is currently used primarily as the Telegram bot layer.

Prefer simple and readable code over complex abstractions.

Do NOT introduce:

- unnecessary classes;
- factories;
- dependency injection frameworks;
- service containers;
- complicated design patterns;
- excessive abstractions;
- unnecessary interfaces or protocols.

If a simple function solves the problem, use a function.
