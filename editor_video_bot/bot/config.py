import os
from dotenv import load_dotenv

load_dotenv()

BOT_TOKEN = os.getenv("BOT_TOKEN")
API_BASE = os.getenv("BACKEND_URL", os.getenv("API_BASE", "http://localhost:8081"))
BOT_SECRET = os.getenv("BOT_SECRET")

# Storage / Media Directory for Scheduled Media Files
MEDIA_DIR = os.getenv("MEDIA_DIR", "/app/media")
STORAGE_DIR = MEDIA_DIR

# Worker polling interval in seconds
WORKER_POLL_INTERVAL = int(os.getenv("WORKER_POLL_INTERVAL", "60"))

# Telegram Bot API Server URL (leave empty for standard Telegram API, or set e.g. http://telegram-bot-api:8081 for local server)
TELEGRAM_LOCAL_SERVER = os.getenv("TELEGRAM_LOCAL_SERVER", "").strip()

