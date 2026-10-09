import os
import re
import logging
from typing import Any, Dict, List, Optional, Tuple, Union

from aiogram import Router, F
from aiogram.filters import Command
from aiogram.types import Message, CallbackQuery

from api.client import get_active_accounts, add_account, clear_account_videos
from config import MEDIA_DIR
from keyboards.acc_kb import (
    get_clear_archive_keyboard,
    get_confirm_clear_keyboard,
    get_account_videos_keyboard,
    get_account_video_detail_keyboard,
    get_account_videos_all_keyboard,
)

logger = logging.getLogger(__name__)

router = Router()


def get_account_directory(account_id: int) -> str:
    """Returns the expected media directory path for an account."""
    return os.path.join(MEDIA_DIR, f"acc_{account_id}")


def get_account_videos_details(account_id: int) -> Dict[str, Any]:
    """
    Inspects the directory for an account (/app/media/acc_<account_id>) and gathers
    detailed statistics: count, filenames, sizes, and total size.
    """
    acc_dir = get_account_directory(account_id)
    valid_exts = (".mp4", ".mov", ".mkv", ".avi", ".webm")

    if not os.path.exists(acc_dir) or not os.path.isdir(acc_dir):
        return {
            "account_id": account_id,
            "dir_path": acc_dir,
            "exists": False,
            "count": 0,
            "files": [],
            "total_size_bytes": 0,
            "total_size_mb": 0.0,
        }

    try:
        raw_files = [
            f for f in os.listdir(acc_dir)
            if not f.startswith(".") and f.lower().endswith(valid_exts)
        ]
        raw_files.sort()

        file_details = []
        total_size = 0
        for fname in raw_files:
            fpath = os.path.join(acc_dir, fname)
            try:
                sz = os.path.getsize(fpath)
            except OSError:
                sz = 0
            total_size += sz
            file_details.append({
                "name": fname,
                "size_bytes": sz,
                "size_mb": round(sz / (1024 * 1024), 2),
            })

        return {
            "account_id": account_id,
            "dir_path": acc_dir,
            "exists": True,
            "count": len(file_details),
            "files": file_details,
            "total_size_bytes": total_size,
            "total_size_mb": round(total_size / (1024 * 1024), 2),
        }
    except Exception as e:
        logger.error(f"Error inspecting directory {acc_dir}: {e}", exc_info=True)
        return {
            "account_id": account_id,
            "dir_path": acc_dir,
            "exists": True,
            "count": 0,
            "files": [],
            "total_size_bytes": 0,
            "total_size_mb": 0.0,
            "error": str(e),
        }


def count_account_videos(account_id: int) -> int:
    """Counts video files in the account directory /app/media/acc_<account_id>."""
    details = get_account_videos_details(account_id)
    return details["count"]


ORDINAL_MAP = {
    "1": 1, "1-й": 1, "1й": 1, "первый": 1, "первого": 1, "первом": 1, "первому": 1, "first": 1, "1st": 1,
    "2": 2, "2-й": 2, "2й": 2, "второй": 2, "второго": 2, "втором": 2, "второму": 2, "second": 2, "2nd": 2,
    "3": 3, "3-й": 3, "3й": 3, "третий": 3, "третьего": 3, "третьем": 3, "третьему": 3, "third": 3, "3rd": 3,
    "4": 4, "4-й": 4, "4й": 4, "четвертый": 4, "четвертого": 4, "четвертом": 4, "четвертому": 4, "fourth": 4, "4th": 4,
    "5": 5, "5-й": 5, "5й": 5, "пятый": 5, "пятого": 5, "пятом": 5, "пятому": 5, "fifth": 5, "5th": 5,
    "6": 6, "6-й": 6, "6й": 6, "шестой": 6, "шестого": 6, "шестом": 6, "шестому": 6, "sixth": 6, "6th": 6,
    "7": 7, "7-й": 7, "7й": 7, "седьмой": 7, "седьмого": 7, "седьмом": 7, "седьмому": 7, "seventh": 7, "7th": 7,
    "8": 8, "8-й": 8, "8й": 8, "восьмой": 8, "восьмого": 8, "восьмом": 8, "восьмому": 8, "eighth": 8, "8th": 8,
    "9": 9, "9-й": 9, "9й": 9, "девятый": 9, "девятого": 9, "девятом": 9, "девятому": 9, "ninth": 9, "9th": 9,
    "10": 10, "10-й": 10, "10й": 10, "десятый": 10, "десятого": 10, "десятом": 10, "десятому": 10, "tenth": 10, "10th": 10,
}


def resolve_account_info(
    account_selector: Union[int, str],
    accounts: Optional[List[Dict[str, Any]]] = None,
) -> Tuple[int, Optional[Dict[str, Any]], Optional[int]]:
    """
    Resolves an account selector into (account_id, account_data, order_index_1based).
    Supports:
    - Ordinals: 'первый' -> 1st account, 'второй' -> 2nd account
    - Numbers: 1, 2, '1', '2'
    - Folder names: 'acc_1', 'acc_2'
    - Account names: matching acc['name']
    """
    clean_sel = str(account_selector).strip().lower()
    order_idx = ORDINAL_MAP.get(clean_sel)

    if accounts:
        # 1. Match by ordinal word if valid index in accounts list
        if order_idx is not None and 1 <= order_idx <= len(accounts):
            acc = accounts[order_idx - 1]
            return acc["id"], acc, order_idx

        # 2. Check if clean_sel is a plain integer (could be account ID or list index)
        if clean_sel.isdigit():
            val = int(clean_sel)
            # Prioritize matching by exact DB account id
            acc_by_id = next((a for a in accounts if a.get("id") == val), None)
            if acc_by_id:
                idx = accounts.index(acc_by_id) + 1
                return acc_by_id["id"], acc_by_id, idx
            # Next, if val is a valid 1-based index (e.g. 1st, 2nd in list)
            if 1 <= val <= len(accounts):
                acc = accounts[val - 1]
                return acc["id"], acc, val
            return val, None, None

        # 3. Match folder format "acc_<id>"
        if clean_sel.startswith("acc_"):
            num_part = clean_sel.replace("acc_", "")
            if num_part.isdigit():
                acc_id = int(num_part)
                acc = next((a for a in accounts if a.get("id") == acc_id), None)
                idx = (accounts.index(acc) + 1) if acc else None
                return acc_id, acc, idx

        # 4. Match by name
        acc = next((a for a in accounts if a.get("name", "").strip().lower() == clean_sel), None)
        if acc:
            idx = accounts.index(acc) + 1
            return acc["id"], acc, idx

    # Fallback if accounts list is not available or account not found in DB
    if order_idx is not None:
        return order_idx, None, order_idx

    match = re.search(r"\d+", str(account_selector))
    if match:
        acc_id = int(match.group())
        return acc_id, None, None

    return 1, None, None


def check_and_report_account_videos(
    account_selector: Union[int, str],
    accounts: Optional[List[Dict[str, Any]]] = None,
    print_output: bool = True,
) -> Dict[str, Any]:
    """
    Looks at and writes ('смотрит и пишет') the number of videos and file details
    in the directory of the specified account (e.g. 1st account, 2nd, etc.).

    Args:
        account_selector: Account ID (1, 2), ordinal ('первый', 'второй', '1-й', '2-й'),
                          folder name ('acc_1'), or account name.
        accounts: Optional pre-fetched list of accounts.
        print_output: If True, writes the result to stdout (console) and logs.

    Returns:
        Dict with keys: account_id, account_name, order_index, count, dir_path, files, total_size_mb, message_text
    """
    account_id, account, order_idx = resolve_account_info(account_selector, accounts)
    details = get_account_videos_details(account_id)

    acc_name = account["name"] if account else f"Аккаунт #{account_id}"
    count = details["count"]
    total_mb = details["total_size_mb"]
    order_str = f", {order_idx}-й аккаунт" if order_idx else ""

    text = (
        f"📱 **Аккаунт:** {acc_name} (ID `#{account_id}`{order_str})\n"
        f"📂 **Директория:** `acc_{account_id}`\n"
        f"🎬 **Количество видео в директории:** `{count}`\n"
        f"📦 **Общий объем:** `{total_mb}` MB\n"
    )

    if account and account.get("publish_time"):
        pub_times = account.get("publish_time") or "13:00"
        slots_count = len([t for t in pub_times.split(",") if t.strip()])
        text += f"🕒 **Расписание публикаций ({slots_count}/день):** `{pub_times}`\n"

    if details["files"]:
        text += "\n🎞 **Список видео в директории:**\n"
        for i, f in enumerate(details["files"][:10], 1):
            text += f"  {i}. `{f['name']}` ({f['size_mb']} MB)\n"
        if len(details["files"]) > 10:
            text += f"  *... и ещё {len(details['files']) - 10} видео*\n"
    elif not details["exists"]:
        text += "\n⚠️ *Директория пока не создана на диске.*"
    else:
        text += "\nℹ️ *В директории сейчас нет готовых видеороликов.*"

    if print_output:
        summary_log = (
            f"[Account Videos] Аккаунт #{account_id} ('{acc_name}'): "
            f"{count} видео в директории {details['dir_path']} ({total_mb} MB)"
        )
        print(summary_log)
        for f in details["files"][:10]:
            print(f"   - {f['name']} ({f['size_mb']} MB)")
        logger.info(summary_log)

    return {
        "account_id": account_id,
        "account_name": acc_name,
        "order_index": order_idx,
        "account": account,
        "count": count,
        "dir_path": details["dir_path"],
        "exists": details["exists"],
        "files": details["files"],
        "total_size_mb": total_mb,
        "message_text": text,
    }


def write_account_videos_count(account_selector: Union[int, str]) -> str:
    """
    Convenience function: inspects the directory and writes the video count to stdout,
    returning the formatted text message.
    """
    res = check_and_report_account_videos(account_selector, print_output=True)
    return res["message_text"]


async def get_and_report_account_videos(
    account_selector: Union[int, str],
    print_output: bool = True,
) -> Dict[str, Any]:
    """
    Asynchronous version that fetches active accounts from the backend API first,
    then inspects and writes the video count for the specified account.
    """
    accounts = None
    try:
        accounts = await get_active_accounts()
    except Exception as e:
        logger.warning(f"Could not fetch accounts from backend API: {e}")

    return check_and_report_account_videos(account_selector, accounts=accounts, print_output=print_output)


@router.message(Command("accounts"))
@router.message(F.text == "📱 Список аккаунтов")
async def cmd_accounts(message: Message):
    """Lists all active TikTok accounts with buttons to view each account's videos."""
    try:
        accounts = await get_active_accounts()
        if not accounts:
            await message.answer(
                "ℹ️ В базе нет активных аккаунтов TikTok.\nДобавьте командой:\n`/add_account <Имя> <Путь_к_кукам> [Прокси] [Время_публикаций]`",
                parse_mode="Markdown",
            )
            return

        video_counts = {acc["id"]: count_account_videos(acc["id"]) for acc in accounts}
        text = "📱 **Список аккаунтов TikTok:**\n\n"
        for idx, acc in enumerate(accounts, 1):
            proxy = acc.get("proxy_url") or "Без прокси"
            pub_times = acc.get("publish_time") or "13:00"
            slots_count = len([t for t in pub_times.split(",") if t.strip()])
            video_cnt = video_counts.get(acc["id"], 0)
            text += (
                f"• **#{acc['id']} {acc['name']}** ({idx}-й аккаунт)\n"
                f"  🎬 Видео в директории: `{video_cnt}`\n"
                f"  🕒 Время публикаций ({slots_count} в день): `{pub_times}`\n"
                f"  🌐 Прокси: `{proxy}`\n"
                f"  🍪 Куки: `{acc['cookies_path']}`\n\n"
            )

        text += "👇 Нажмите на кнопку нужного аккаунта, чтобы открыть его видео:"
        kb = get_account_videos_keyboard(accounts, video_counts=video_counts)
        await message.answer(text, reply_markup=kb, parse_mode="Markdown")
    except Exception as e:
        logger.error(f"Error listing accounts: {e}", exc_info=True)
        await message.answer(f"❌ Ошибка получения списка аккаунтов: {e}")


@router.callback_query(F.data == "menu:accounts")
async def cb_menu_accounts(callback: CallbackQuery):
    try:
        accounts = await get_active_accounts()
        if not accounts:
            if callback.message:
                await callback.message.edit_text("ℹ️ В базе нет активных аккаунтов TikTok.")
            await callback.answer()
            return

        video_counts = {acc["id"]: count_account_videos(acc["id"]) for acc in accounts}
        text = "📱 **Список аккаунтов TikTok:**\n\n"
        for idx, acc in enumerate(accounts, 1):
            pub_times = acc.get("publish_time") or "13:00"
            slots_count = len([t for t in pub_times.split(",") if t.strip()])
            video_cnt = video_counts.get(acc["id"], 0)
            text += (
                f"• **#{acc['id']} {acc['name']}** ({idx}-й аккаунт)\n"
                f"  🎬 Видео: `{video_cnt}` | 🕒 Слотов: {slots_count}\n"
            )

        text += "\n👇 Нажмите кнопку аккаунта для детальной информации:"
        kb = get_account_videos_keyboard(accounts, video_counts=video_counts)
        if callback.message:
            await callback.message.edit_text(text, reply_markup=kb, parse_mode="Markdown")
        await callback.answer()
    except Exception as e:
        logger.error(f"Error in cb_menu_accounts: {e}", exc_info=True)
        await callback.answer("Ошибка получения аккаунтов", show_alert=True)


@router.callback_query(F.data == "menu:clear_archive")
async def cb_menu_clear_archive(callback: CallbackQuery):
    try:
        accounts = await get_active_accounts()
        if not accounts:
            if callback.message:
                await callback.message.edit_text("ℹ️ В базе нет активных аккаунтов TikTok.")
            await callback.answer()
            return

        kb = get_clear_archive_keyboard(accounts)
        if callback.message:
            await callback.message.edit_text(
                "🗑 **Очистка архива видео аккаунта**\n\n"
                "Выберите аккаунт TikTok, из архива которого вы хотите удалить все нарезки:",
                reply_markup=kb,
                parse_mode="Markdown",
            )
        await callback.answer()
    except Exception as e:
        logger.error(f"Error in cb_menu_clear_archive: {e}", exc_info=True)
        await callback.answer("Ошибка получения аккаунтов", show_alert=True)


@router.message(Command("videos_count", "videos", "account_videos", "acc_videos"))
@router.message(F.text == "📁 Видео по аккаунтам")
@router.message(F.text == "📊 Сводка всех видео")
async def cmd_account_videos(message: Message):
    """
    Displays buttons for all accounts fetched automatically from DB.
    Clicking any button shows that account's video count and files immediately.
    """
    text_content = (message.text or "").strip()
    is_summary_btn = (text_content == "📊 Сводка всех видео")
    parts = text_content.split(maxsplit=1)
    selector = parts[1].strip() if len(parts) > 1 else None

    try:
        accounts = await get_active_accounts()
    except Exception as e:
        logger.error(f"Error in cmd_account_videos getting accounts: {e}", exc_info=True)
        accounts = []

    if is_summary_btn or (selector and selector.lower() in ("all", "все", "всё")):
        if not accounts:
            await message.answer("ℹ️ В базе нет активных аккаунтов TikTok.", parse_mode="Markdown")
            return

        total_videos = 0
        total_mb = 0.0
        text = "📊 **Количество видео по всем аккаунтам:**\n\n"
        for idx, acc in enumerate(accounts, 1):
            details = get_account_videos_details(acc["id"])
            cnt = details["count"]
            total_videos += cnt
            total_mb += details["total_size_mb"]
            text += f"• **#{acc['id']} {acc['name']}** ({idx}-й акк): `{cnt}` видео ({details['total_size_mb']} MB)\n"

        text += f"\n📦 **Всего видео на сервере:** `{total_videos}` ({round(total_mb, 2)} MB)"
        kb = get_account_videos_all_keyboard()
        await message.answer(text, reply_markup=kb, parse_mode="Markdown")
        return

    if selector:
        report = check_and_report_account_videos(selector, accounts=accounts, print_output=True)
        kb = get_account_video_detail_keyboard(report["account_id"])
        await message.answer(report["message_text"], reply_markup=kb, parse_mode="Markdown")
        return

    if not accounts:
        await message.answer(
            "ℹ️ В базе нет активных аккаунтов TikTok.\nДобавьте аккаунт с помощью `/add_account`.",
            parse_mode="Markdown",
        )
        return

    video_counts = {acc["id"]: count_account_videos(acc["id"]) for acc in accounts}
    kb = get_account_videos_keyboard(accounts, video_counts=video_counts)
    await message.answer(
        "📁 **Видео в директориях аккаунтов**\n\n"
        "Нажмите на кнопку аккаунта ниже, чтобы сразу открыть информацию о видеороликах:",
        reply_markup=kb,
        parse_mode="Markdown",
    )


@router.callback_query(F.data == "acc_videos:close")
async def cb_acc_videos_close(callback: CallbackQuery):
    if callback.message:
        await callback.message.delete()
    await callback.answer()


@router.callback_query(F.data == "acc_videos:menu")
async def cb_acc_videos_menu(callback: CallbackQuery):
    try:
        accounts = await get_active_accounts()
        if not accounts:
            if callback.message:
                await callback.message.edit_text("ℹ️ В базе нет активных аккаунтов TikTok.")
            await callback.answer()
            return

        video_counts = {acc["id"]: count_account_videos(acc["id"]) for acc in accounts}
        kb = get_account_videos_keyboard(accounts, video_counts=video_counts)
        if callback.message:
            await callback.message.edit_text(
                "📁 **Видео в директориях аккаунтов**\n\n"
                "Нажмите на кнопку аккаунта ниже, чтобы сразу открыть информацию о видеороликах:",
                reply_markup=kb,
                parse_mode="Markdown",
            )
        await callback.answer()
    except Exception as e:
        logger.error(f"Error returning to video menu: {e}", exc_info=True)
        await callback.answer("Ошибка при загрузке меню", show_alert=True)


@router.callback_query(F.data.startswith("acc_videos:view:"))
async def cb_acc_videos_view(callback: CallbackQuery):
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
        accounts = await get_active_accounts()
    except Exception as e:
        logger.error(f"Error fetching accounts: {e}")
        accounts = []

    try:
        report = check_and_report_account_videos(account_id, accounts=accounts, print_output=True)
        kb = get_account_video_detail_keyboard(account_id)

        if callback.message:
            await callback.message.edit_text(
                report["message_text"],
                reply_markup=kb,
                parse_mode="Markdown",
            )
        await callback.answer(f"Видео: {report['count']}")
    except Exception as e:
        logger.error(f"Error fetching account videos: {e}", exc_info=True)
        await callback.answer("Ошибка при подсчете видео", show_alert=True)


@router.callback_query(F.data == "acc_videos:all")
async def cb_acc_videos_all(callback: CallbackQuery):
    try:
        accounts = await get_active_accounts()
        if not accounts:
            if callback.message:
                await callback.message.edit_text("ℹ️ В базе нет активных аккаунтов TikTok.")
            await callback.answer()
            return

        total_videos = 0
        total_mb = 0.0
        text = "📊 **Количество видео по всем аккаунтам:**\n\n"
        for idx, acc in enumerate(accounts, 1):
            details = get_account_videos_details(acc["id"])
            cnt = details["count"]
            total_videos += cnt
            total_mb += details["total_size_mb"]
            text += f"• **#{acc['id']} {acc['name']}** ({idx}-й акк): `{cnt}` видео ({details['total_size_mb']} MB)\n"

        text += f"\n📦 **Всего видео на сервере:** `{total_videos}` ({round(total_mb, 2)} MB)"

        kb = get_account_videos_all_keyboard()
        if callback.message:
            await callback.message.edit_text(
                text,
                reply_markup=kb,
                parse_mode="Markdown",
            )
        await callback.answer()
    except Exception as e:
        logger.error(f"Error showing all account video counts: {e}", exc_info=True)
        await callback.answer("Ошибка при получении данных", show_alert=True)


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


if __name__ == "__main__":
    import sys
    arg = sys.argv[1] if len(sys.argv) > 1 else "1"
    print(f"--- Проверка директории аккаунта: '{arg}' ---")
    report = check_and_report_account_videos(arg, print_output=True)
    print("\n--- Результат сообщения: ---")
    print(report["message_text"])



