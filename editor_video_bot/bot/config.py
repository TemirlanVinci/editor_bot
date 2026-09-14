import os
from dotenv import load_dotenv

load_dotenv()

BOT_TOKEN = os.getenv("BOT_TOKEN")
API_BASE = os.getenv("BACKEND_URL", os.getenv("API_BASE", "http://localhost:8081"))
BOT_SECRET = os.getenv("BOT_SECRET")

# Убрали PRODUCTS_LIMIT = 5

