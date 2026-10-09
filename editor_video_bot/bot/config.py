import os
from dotenv import load_dotenv

load_dotenv()

BOT_TOKEN = os.getenv("BOT_TOKEN")
API_BASE = os.getenv("BACKEND_URL", os.getenv("API_BASE", "http://localhost:8081"))
BOT_SECRET = os.getenv("BOT_SECRET")

# Storage / Media Directory for Scheduled Media Files
MEDIA_DIR = os.getenv("MEDIA_DIR")
if not MEDIA_DIR:
    if os.path.exists("/app/media"):
        MEDIA_DIR = "/app/media"
    else:
        MEDIA_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "media"))
STORAGE_DIR = MEDIA_DIR

# Worker polling interval in seconds
WORKER_POLL_INTERVAL = int(os.getenv("WORKER_POLL_INTERVAL", "60"))

# Telegram Bot API Server URL (leave empty for standard Telegram API, or set e.g. http://telegram-bot-api:8081 for local server)
TELEGRAM_LOCAL_SERVER = os.getenv("TELEGRAM_LOCAL_SERVER", "").strip()

# Admin Whitelist (empty allows all users, or comma-separated Telegram user IDs)
ADMIN_IDS_RAW = os.getenv("ADMIN_IDS", "").strip()
ADMIN_IDS: set[int] = {
    int(x.strip()) for x in ADMIN_IDS_RAW.split(",") if x.strip().isdigit()
}

