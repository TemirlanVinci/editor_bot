import asyncio
import logging

from aiogram import Bot, Dispatcher, F
from aiogram.types import CallbackQuery
from aiogram.fsm.storage.memory import MemoryStorage
from aiogram.client.default import DefaultBotProperties

from config import BOT_TOKEN
from api.client import init_session, close_session
# from filters import refresh_admins

from handlers import admin, cut, download, start

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

bot = Bot(token=BOT_TOKEN, default=DefaultBotProperties(parse_mode="HTML"))
dp = Dispatcher(storage=MemoryStorage())

dp.include_router(start.router)
dp.include_router(admin.router)
dp.include_router(cut.router)
dp.include_router(download.router)

@dp.callback_query(F.data == "noop")
async def cb_noop(cb: CallbackQuery) -> None:
    await cb.answer()

async def on_startup(bot: Bot) -> None:
    await init_session()
    # await refresh_admins()
    
    # Сбрасываем вебхук на случай, если Telegram его запомнил
    await bot.delete_webhook(drop_pending_updates=True)
    logger.info("Бот запущен (Long Polling)")

async def on_shutdown(bot: Bot) -> None:
    await close_session()
    await bot.session.close()
    logger.info("Бот корректно остановлен")

async def main() -> None:
    dp.startup.register(on_startup)
    dp.shutdown.register(on_shutdown)
    
    await dp.start_polling(bot)

if __name__ == "__main__":
    asyncio.run(main())