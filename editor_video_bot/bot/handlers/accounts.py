import logging
from aiogram import Router, F
from aiogram.filters import Command
from aiogram.types import Message, CallbackQuery

from api.client import get_active_accounts, add_account, clear_account_videos
from keyboards.acc_kb import get_clear_archive_keyboard, get_confirm_clear_keyboard

logger = logging.getLogger(__name__)

router = Router()


@router.message(Command("accounts"))
async def cmd_accounts(message: Message):
    """Lists all active TikTok accounts."""
    try:
        accounts = await get_active_accounts()
        if not accounts:
            await message.answer(
                "ℹ️ В базе нет активных аккаунтов TikTok.\nДобавьте командой:\n`/add_account <Имя> <Путь_к_кукам> [Прокси] [Время_публикаций]`",
                parse_mode="Markdown",
            )
            return

        text = "📱 **Список аккаунтов TikTok:**\n\n"
        for acc in accounts:
            proxy = acc.get("proxy_url") or "Без прокси"
            pub_times = acc.get("publish_time") or "13:00"
            slots_count = len([t for t in pub_times.split(",") if t.strip()])
            text += (
                f"• **#{acc['id']} {acc['name']}**\n"
                f"  🕒 Время публикаций ({slots_count} в день): `{pub_times}`\n"
                f"  🌐 Прокси: `{proxy}`\n"
                f"  🍪 Куки: `{acc['cookies_path']}`\n\n"
            )

        await message.answer(text, parse_mode="Markdown")
    except Exception as e:
        logger.error(f"Error listing accounts: {e}", exc_info=True)
        await message.answer(f"❌ Ошибка получения списка аккаунтов: {e}")


@router.message(Command("add_account"))
async def cmd_add_account(message: Message):
    """
    Adds a new TikTok account.
    Format: /add_account <name> <cookies_path> [proxy_url] [publish_times]
    Example: /add_account Account_RU /app/media/cookies_acc1.json http://proxy:8080 10:00,15:00,20:00
    """
    args = message.text.split(maxsplit=4) if message.text else []
    if len(args) < 3:
        await message.answer(
            "Пожалуйста, укажите имя и путь к файлу куки.\n"
            "Пример (1 видео в день в 13:00):\n`/add_account Аккаунт_1 /app/media/cookies_acc1.json http://user:pass@ip:port 13:00`\n\n"
            "Пример (3 видео в день):\n`/add_account Аккаунт_2 /app/media/cookies_acc2.json http://user:pass@ip:port 10:00,15:00,20:00`",
            parse_mode="Markdown",
        )
        return

    name = args[1].strip()
    cookies_path = args[2].strip()
    proxy_url = args[3].strip() if len(args) > 3 else ""
    publish_time = args[4].strip() if len(args) > 4 else "13:00"

    try:
        acc = await add_account(
            name=name,
            cookies_path=cookies_path,
            proxy_url=proxy_url,
            publish_time=publish_time,
        )
        await message.answer(
            f"✅ Аккаунт **{acc['name']}** (ID #{acc['id']}) успешно добавлен!\n"
            f"🕒 Слоты публикаций: `{acc.get('publish_time', publish_time)}`",
            parse_mode="Markdown",
        )
    except Exception as e:
        logger.error(f"Error adding account: {e}", exc_info=True)
        await message.answer(f"❌ Ошибка при добавлении аккаунта: {e}")


@router.message(Command("clear_archive", "clear_videos", "clear_acc"))
async def cmd_clear_archive(message: Message):
    """
    Shows inline keyboard with active accounts to clear archived video clips.
    """
    try:
        accounts = await get_active_accounts()
        if not accounts:
            await message.answer(
                "ℹ️ В базе нет активных аккаунтов TikTok.",
                parse_mode="Markdown",
            )
            return

        kb = get_clear_archive_keyboard(accounts)
        await message.answer(
            "🗑 **Очистка архива видео аккаунта**\n\n"
            "Выберите аккаунт TikTok, из архива которого вы хотите удалить все нарезки:",
            reply_markup=kb,
            parse_mode="Markdown",
        )
    except Exception as e:
        logger.error(f"Error preparing clear archive UI: {e}", exc_info=True)
        await message.answer(f"❌ Ошибка при получении аккаунтов: {e}")


@router.callback_query(F.data == "clear_acc:cancel")
async def cb_clear_acc_cancel(callback: CallbackQuery):
    if callback.message:
        await callback.message.edit_text("❌ Операция очистки архива отменена.")
    await callback.answer("Отменено.")


@router.callback_query(F.data.startswith("clear_acc:select:"))
async def cb_clear_acc_select(callback: CallbackQuery):
    parts = callback.data.split(":")
    if len(parts) < 3:
        await callback.answer("Неверные данные колбэка", show_alert=True)
        return

    try:
        account_id = int(parts[2])
    except ValueError:
        await callback.answer("Неверный ID аккаунта", show_alert=True)
        return

    kb = get_confirm_clear_keyboard(account_id)
    if callback.message:
        await callback.message.edit_text(
            f"⚠️ **Подтверждение удаления**\n\n"
            f"Вы действительно хотите безвозвратно удалить **ВСЕ видео** и задачи публикаций для аккаунта **#{account_id}**?",
            reply_markup=kb,
            parse_mode="Markdown",
        )
    await callback.answer()


@router.callback_query(F.data.startswith("clear_acc:confirm:"))
async def cb_clear_acc_confirm(callback: CallbackQuery):
    parts = callback.data.split(":")
    if len(parts) < 3:
        await callback.answer("Неверные данные колбэка", show_alert=True)
        return

    try:
        account_id = int(parts[2])
    except ValueError:
        await callback.answer("Неверный ID аккаунта", show_alert=True)
        return

    try:
        res = await clear_account_videos(account_id)
        deleted_count = res.get("deleted_count", 0)

        if callback.message:
            await callback.message.edit_text(
                f"✅ **Архив аккаунта #{account_id} успешно очищен!**\n\n"
                f"🗑 Удалено клипов/задач: **{deleted_count}**.",
                parse_mode="Markdown",
            )
        await callback.answer("Архив очищен!")
    except Exception as e:
        logger.error(f"Error clearing account archive: {e}", exc_info=True)
        if callback.message:
            await callback.message.edit_text(f"❌ Ошибка при очистке архива: {e}")
        await callback.answer("Ошибка при очистке", show_alert=True)


