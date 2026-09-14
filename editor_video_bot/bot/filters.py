'''import time 
from aiogram.filters import BaseFilter
from aiogram.types import Message, CallbackQuery

from api.admin import get_admin_list

REFRESH_INTERVAL = 300

_admin_ids: set[int] = set()
_last_refresh: float = 0.0


async def refresh_admins() -> None:
    global _admin_ids, _last_refresh

    data = await get_admin_list()
    if data and "admins" in data:
        _admin_ids = {
            admin["telegram_id"]
            for admin in data["admins"]
            if admin.get("is_active")
        }
    _last_refresh = time.monotonic()


async def _ensure_fresh() -> None:
    if time.monotonic() - _last_refresh > REFRESH_INTERVAL:
        await refresh_admins()

    
class IsAdmin(BaseFilter):
    async def __call__(self, event: Message | CallbackQuery) -> bool:
        await _ensure_fresh()
        return event.from_user.id in _admin_ids'''