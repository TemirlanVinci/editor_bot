import os
import re
import shutil
import uuid
import zipfile
import logging
from typing import List

from aiogram import Router, F
from aiogram.filters import Command
from aiogram.types import Message, CallbackQuery

from api.client import (
    download_video,
    cut_video,
    get_active_accounts,
    schedule_clips,
)
from config import MEDIA_DIR
from keyboards.acc_kb import get_account_selection_keyboard

logger = logging.getLogger(__name__)

router = Router()


def extract_number(filename: str) -> int:
    """Extracts the first number in a filename for sorting."""
    match = re.search(r"\d+", filename)
    return int(match.group()) if match else 0


@router.message(Command("acc"))
async def cmd_acc(message: Message):
    """
    Handler for command /acc <url>
    1. Downloads video from backend endpoint POST /api/v1/video/download
    2. Cuts video into clips via POST /api/v1/video/cut
    3. Displays inline keyboard to select target TikTok account or cancel
    """
    args = message.text.split(maxsplit=1) if message.text else []
    if len(args) < 2 or not args[1].strip():
        await message.answer(
            "Пожалуйста, укажите ссылку на видео.\nПример: `/acc https://www.youtube.com/watch?v=...`",
            parse_mode="Markdown",
        )
        return

    url = args[1].strip()
    status_msg = await message.answer("⏳ Скачивание видео...")

    job_id = f"{message.chat.id}_{uuid.uuid4().hex[:8]}"
    job_tmp_dir = os.path.join(MEDIA_DIR, "tmp", f"job_{job_id}")
    os.makedirs(job_tmp_dir, exist_ok=True)

    downloaded_path = os.path.join(job_tmp_dir, "downloaded.mp4")
    zip_path = os.path.join(job_tmp_dir, "result.zip")
    extracted_dir = os.path.join(job_tmp_dir, "clips")

    try:
        # Step 1: Download video
        video_bytes = await download_video(url)
        with open(downloaded_path, "wb") as f:
            f.write(video_bytes)

        # Step 2: Cut video
        await status_msg.edit_text("✂️ Нарезка и обработка клипов...")
        await cut_video(downloaded_path, zip_path)

        # Extract ZIP clips
        os.makedirs(extracted_dir, exist_ok=True)
        with zipfile.ZipFile(zip_path, "r") as zip_ref:
            zip_ref.extractall(extracted_dir)

        # Collect clip files
        clip_files = [
            f for f in os.listdir(extracted_dir)
            if os.path.isfile(os.path.join(extracted_dir, f)) and not f.startswith(".")
        ]
        clip_files.sort(key=extract_number)

        if not clip_files:
            await status_msg.edit_text("❌ При обработке видео не создано ни одного клипа.")
            shutil.rmtree(job_tmp_dir, ignore_errors=True)
            return

        # Fetch active accounts via backend API
        accounts = await get_active_accounts()
        if not accounts:
            await status_msg.edit_text(
                "⚠️ Активные аккаунты TikTok не найдены в БД. Зарегистрируйте аккаунт с помощью `/add_account`."
            )
            shutil.rmtree(job_tmp_dir, ignore_errors=True)
            return

        # Render Account Selection UI
        kb = get_account_selection_keyboard(accounts, job_id)
        await status_msg.edit_text(
            f"✂️ Успешно создано клипов: {len(clip_files)}.\nВыберите аккаунт TikTok для публикации:",
            reply_markup=kb,
        )

    except Exception as e:
        logger.error(f"Error in /acc pipeline: {e}", exc_info=True)
        await status_msg.edit_text(f"❌ Ошибка обработки запроса: {e}")
        shutil.rmtree(job_tmp_dir, ignore_errors=True)


@router.callback_query(F.data.startswith("pub:"))
async def handle_account_callback(callback: CallbackQuery):
    """
    Handles callbacks for account selection or cancellation:
    - pub:cancel:<job_id>
    - pub:<account_id>:<job_id>
    """
    parts = callback.data.split(":")
    if len(parts) < 3:
        await callback.answer("Неверные данные колбэка", show_alert=True)
        return

    action = parts[1]
    job_id = parts[2]
    job_tmp_dir = os.path.join(MEDIA_DIR, "tmp", f"job_{job_id}")

    # Handle Cancel
    if action == "cancel":
        shutil.rmtree(job_tmp_dir, ignore_errors=True)
        fallback_tmp = os.path.join("/tmp", f"job_{job_id}")
        shutil.rmtree(fallback_tmp, ignore_errors=True)

        if callback.message:
            await callback.message.edit_text("Отменено. Временные файлы удалены.")
        await callback.answer("Отменено.")
        return

    # Handle Account Selection
    try:
        account_id = int(action)
    except ValueError:
        await callback.answer("Неверный ID аккаунта", show_alert=True)
        return

    try:
        # Call backend API to schedule clips
        res = await schedule_clips(account_id=account_id, job_id=job_id)
        scheduled_count = res.get("scheduled_count", 0)
        first_post_str = res.get("first_scheduled_at", "неизвестно")

        if callback.message:
            await callback.message.edit_text(
                f"✅ Успешно запланировано {scheduled_count} роликов для Аккаунта #{account_id}!\n"
                f"📅 Первый пост: {first_post_str}."
            )

        await callback.answer("Публикации запланированы!")

    except Exception as e:
        logger.error(f"Error scheduling clips via backend: {e}", exc_info=True)
        if callback.message:
            await callback.message.edit_text(f"❌ Ошибка при планировании публикаций: {e}")
        await callback.answer("Ошибка при планировании", show_alert=True)
