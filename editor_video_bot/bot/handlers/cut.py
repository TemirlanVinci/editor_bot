import os
import re
import shutil
import tempfile
import zipfile
import asyncio
import logging
from aiogram import Router, F, Bot
from aiogram.exceptions import TelegramBadRequest
from aiogram.filters import Command
from aiogram.types import Message, FSInputFile
from aiogram.fsm.context import FSMContext
from aiogram.fsm.state import StatesGroup, State

from config import TELEGRAM_LOCAL_SERVER
from api.client import cut_video, download_video_to_file

logger = logging.getLogger(__name__)

router = Router()

URL_PATTERN = re.compile(r'https?://[^\s]+')

def extract_number(filename: str) -> int:
    match = re.search(r'\d+', filename)
    return int(match.group()) if match else 0

class CutStates(StatesGroup):
    waiting_for_video = State()

async def run_cut_and_send(message: Message, status_msg: Message, video_path: str, zip_path: str, extract_dir: str):
    """Common pipeline: sends video to backend cut endpoint, extracts zip and uploads clips back to user."""
    try:
        await status_msg.edit_text("Обрабатываем видео...")

        # 1. Send to backend for cutting
        try:
            await cut_video(video_path, zip_path)
        except asyncio.TimeoutError:
            logger.error("Backend request timed out.")
            await status_msg.edit_text("Превышено время ожидания ответа от сервера (таймаут).")
            return
        except Exception as e:
            logger.error(f"Backend processing failed: {e}")
            await status_msg.edit_text("Ошибка при обработке видео на сервере.")
            return

        # 2. Extract ZIP
        try:
            os.makedirs(extract_dir, exist_ok=True)
            with zipfile.ZipFile(zip_path, 'r') as zip_ref:
                zip_ref.extractall(extract_dir)
        except Exception as e:
            logger.error(f"Failed to extract ZIP: {e}")
            await status_msg.edit_text("Ошибка при распаковке ответа от сервера.")
            return

        # 3. Send fragments
        try:
            files = [f for f in os.listdir(extract_dir) if os.path.isfile(os.path.join(extract_dir, f))]
            files.sort(key=extract_number)

            if not files:
                await status_msg.edit_text("Сервер вернул пустой архив без фрагментов.")
                return

            await status_msg.edit_text("Видео разделено. Отправляю фрагменты...")

            for i, file_name in enumerate(files, 1):
                part_num = extract_number(file_name) or i
                fragment_path = os.path.join(extract_dir, file_name)
                input_file = FSInputFile(fragment_path)
                await message.answer_video(input_file, caption=str(part_num))

            await status_msg.delete()
        except Exception as e:
            logger.error(f"Failed to send video fragments to Telegram: {e}")
            await message.answer("Произошла ошибка при отправке фрагментов в Telegram.")

    except Exception as e:
        logger.error(f"Error during cut pipeline: {e}")
        await status_msg.edit_text("Произошла непредвиденная ошибка при обработке.")

@router.message(Command("cut"))
async def cmd_cut(message: Message, state: FSMContext):
    args = message.text.split(maxsplit=1) if message.text else []
    if len(args) > 1 and URL_PATTERN.search(args[1]):
        url = URL_PATTERN.search(args[1]).group(0)
        await state.clear()
        await process_url_cut(message, url)
        return

    await message.answer(
        "Отправь видеофайл (до 20 МБ) или ссылку на YouTube видео в чат."
    )
    await state.set_state(CutStates.waiting_for_video)

async def process_url_cut(message: Message, url: str):
    status_msg = await message.answer("📥 Видео скачивается по ссылке...")
    temp_dir = tempfile.mkdtemp()
    video_path = os.path.join(temp_dir, f"video_{message.message_id}.mp4")
    zip_path = os.path.join(temp_dir, f"result_{message.message_id}.zip")
    extract_dir = os.path.join(temp_dir, "fragments")

    try:
        try:
            await download_video_to_file(url, video_path)
        except Exception as e:
            logger.error(f"Failed to download video from URL {url}: {e}")
            await status_msg.edit_text("❌ Ошибка при скачивании видео по ссылке. Убедитесь, что ссылка валидна.")
            return

        await run_cut_and_send(message, status_msg, video_path, zip_path, extract_dir)
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

@router.message(CutStates.waiting_for_video, F.text)
async def handle_text_url(message: Message, state: FSMContext):
    if URL_PATTERN.search(message.text):
        url = URL_PATTERN.search(message.text).group(0)
        await state.clear()
        await process_url_cut(message, url)
    else:
        await message.answer("Это не ссылка на видео. Отправь видеофайл или ссылку на YouTube.")

@router.message(CutStates.waiting_for_video, F.video | F.document)
@router.message(F.video | F.document)
async def handle_video(message: Message, state: FSMContext, bot: Bot):
    video_obj = None
    if message.video:
        video_obj = message.video
    elif message.document:
        mime = message.document.mime_type or ""
        fname = message.document.file_name or ""
        if mime.startswith("video/") or fname.lower().endswith(('.mp4', '.mov', '.avi', '.mkv', '.webm', '.m4v')):
            video_obj = message.document
        else:
            if await state.get_state() == CutStates.waiting_for_video:
                await state.clear()
                await message.answer("Этот документ не является видео. Операция отменена.")
            return

    if not video_obj:
        return

    await state.clear()

    # Pre-check file size for Standard Telegram Bot API (limit 20MB)
    if not TELEGRAM_LOCAL_SERVER and video_obj.file_size and video_obj.file_size > 20 * 1024 * 1024:
        size_mb = round(video_obj.file_size / (1024 * 1024), 1)
        await message.answer(
            f"❌ <b>Файл слишком большой ({size_mb} MB).</b>\n\n"
            f"Стандартный сервер Telegram Bot API ограничивает скачивание файлов через ботов до <b>20 MB</b>.\n\n"
            f"💡 <b>Что можно сделать:</b>\n"
            f"1️⃣ Отправить <b>ссылку</b> на YouTube видео (например: <code>/cut https://youtu.be/...</code>)\n"
            f"2️⃣ Или сжать видеофайл до размера менее 20 MB."
        )
        return

    status_msg = await message.answer("Видео получено. Скачиваем...")

    temp_dir = tempfile.mkdtemp()
    video_path = os.path.join(temp_dir, f"video_{message.message_id}.mp4")
    zip_path = os.path.join(temp_dir, f"result_{message.message_id}.zip")
    extract_dir = os.path.join(temp_dir, "fragments")

    try:
        try:
            await bot.download(video_obj, destination=video_path)
        except TelegramBadRequest as e:
            if "file is too big" in str(e).lower():
                await status_msg.edit_text(
                    "❌ <b>Файл слишком большой.</b>\n\n"
                    "Telegram Bot API запрещает ботам скачивать файлы больше 20 MB из чата.\n\n"
                    "💡 Отправь <b>ссылку на YouTube</b> в виде:\n"
                    "<code>/cut https://youtu.be/...</code>"
                )
            else:
                logger.error(f"Failed to download video from Telegram: {e}")
                await status_msg.edit_text(f"Ошибка при скачивании видео из Telegram: {e.message}")
            return
        except Exception as e:
            logger.error(f"Failed to download video from Telegram: {e}")
            await status_msg.edit_text("Ошибка при скачивании видео из Telegram.")
            return

        await run_cut_and_send(message, status_msg, video_path, zip_path, extract_dir)

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)


