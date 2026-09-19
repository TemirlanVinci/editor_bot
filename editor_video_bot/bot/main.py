import asyncio
import logging
from typing import Optional

from aiogram import Bot, Dispatcher, F
from aiogram.types import CallbackQuery
from aiogram.fsm.storage.memory import MemoryStorage
from aiogram.client.default import DefaultBotProperties

from config import BOT_TOKEN
from api.client import init_session, close_session
from handlers import admin, cut, download, start, acc, accounts
from worker import start_worker_loop

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

bot = Bot(token=BOT_TOKEN, default=DefaultBotProperties(parse_mode="HTML"))
dp = Dispatcher(storage=MemoryStorage())

dp.include_router(start.router)
dp.include_router(admin.router)
dp.include_router(cut.router)
dp.include_router(download.router)
dp.include_router(acc.router)
dp.include_router(accounts.router)

worker_task: Optional[asyncio.Task] = None


@dp.callback_query(F.data == "noop")
async def cb_noop(cb: CallbackQuery) -> None:
    await cb.answer()


async def on_startup(bot: Bot) -> None:
    global worker_task
    await init_session()
    await bot.delete_webhook(drop_pending_updates=True)

    # Launch background TikTok upload worker task
    worker_task = asyncio.create_task(start_worker_loop())

    logger.info("Бот и фоновый TikTok Worker успешно запущены!")


async def on_shutdown(bot: Bot) -> None:
    global worker_task
    if worker_task:
        worker_task.cancel()
        try:
            await worker_task
        except asyncio.CancelledError:
            pass

    await close_session()
    await bot.session.close()
    logger.info("Бот корректно остановлен")


async def main() -> None:
    dp.startup.register(on_startup)
    dp.shutdown.register(on_shutdown)

    await dp.start_polling(bot)


if __name__ == "__main__":
    asyncio.run(main())