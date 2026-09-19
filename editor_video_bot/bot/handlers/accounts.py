import logging
from aiogram import Router
from aiogram.filters import Command
from aiogram.types import Message

from api.client import get_active_accounts, add_account

logger = logging.getLogger(__name__)

router = Router()


@router.message(Command("accounts"))
async def cmd_accounts(message: Message):
    """Lists all active TikTok accounts."""
    try:
        accounts = await get_active_accounts()
        if not accounts:
            await message.answer("ℹ️ В базе нет активных аккаунтов TikTok.\nДобавьте командой:\n`/add_account <Имя> <Путь_к_кукам> [Прокси]`", parse_mode="Markdown")
            return

        text = "📱 **Список аккаунтов TikTok:**\n\n"
        for acc in accounts:
            proxy = acc.get("proxy_url") or "Без прокси"
            text += f"• **#{acc['id']} {acc['name']}**\n  🕒 Публикация: {acc['publish_time']}\n  🌐 Прокси: `{proxy}`\n  🍪 Куки: `{acc['cookies_path']}`\n\n"

        await message.answer(text, parse_mode="Markdown")
    except Exception as e:
        logger.error(f"Error listing accounts: {e}", exc_info=True)
        await message.answer(f"❌ Ошибка получения списка аккаунтов: {e}")


@router.message(Command("add_account"))
async def cmd_add_account(message: Message):
    """
    Adds a new TikTok account.
    Format: /add_account <name> <cookies_path> [proxy_url] [publish_time]
    Example: /add_account Account_RU /app/media/cookies_acc1.json http://proxy:8080 13:00
    """
    args = message.text.split(maxsplit=4) if message.text else []
    if len(args) < 3:
        await message.answer(
            "Пожалуйста, укажите имя и путь к файлу куки.\n"
            "Пример:\n`/add_account Аккаунт_1 /app/media/cookies_acc1.json http://user:pass@ip:port 13:00`",
            parse_mode="Markdown",
        )
        return

    name = args[1].strip()
    cookies_path = args[2].strip()
    proxy_url = args[3].strip() if len(args) > 3 else ""
    publish_time = args[4].strip() if len(args) > 4 else "13:00:00"

    try:
        acc = await add_account(
            name=name,
            cookies_path=cookies_path,
            proxy_url=proxy_url,
            publish_time=publish_time,
        )
        await message.answer(
            f"✅ Аккаунт **{acc['name']}** (ID #{acc['id']}) успешно добавлен в БД!",
            parse_mode="Markdown",
        )
    except Exception as e:
        logger.error(f"Error adding account: {e}", exc_info=True)
        await message.answer(f"❌ Ошибка при добавлении аккаунта: {e}")
