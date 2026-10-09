from typing import Any, Awaitable, Callable, Dict
from aiogram import BaseMiddleware
from aiogram.types import CallbackQuery, Message, TelegramObject
from config import ADMIN_IDS


class AdminOnlyMiddleware(BaseMiddleware):
    """
    Restricts access to bot commands and callback queries.
    If ADMIN_IDS is empty, all users are permitted.
    If ADMIN_IDS is populated, only listed user IDs can interact with the bot.
    """

    async def __call__(
        self,
        handler: Callable[[TelegramObject, Dict[str, Any]], Awaitable[Any]],
        event: TelegramObject,
        data: Dict[str, Any],
    ) -> Any:
        if not ADMIN_IDS:
            return await handler(event, data)

        user = getattr(event, "from_user", None)
        if user and user.id in ADMIN_IDS:
            return await handler(event, data)

        if isinstance(event, Message):
            await event.answer("⛔ Доступ ограничен. Вы не авторизованы для использования этого бота.")
        elif isinstance(event, CallbackQuery):
            await event.answer("⛔ Доступ ограничен", show_alert=True)
        return None