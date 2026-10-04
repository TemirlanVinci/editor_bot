import os
import re
import shutil
import tempfile
import zipfile
import asyncio
import logging
from typing import Optional, Tuple
from aiogram import Router, F, Bot
from aiogram.exceptions import TelegramBadRequest
from aiogram.filters import Command
from aiogram.types import Message, FSInputFile, CallbackQuery
from aiogram.fsm.context import FSMContext
from aiogram.fsm.state import StatesGroup, State

from config import TELEGRAM_LOCAL_SERVER
from api.client import cut_video, download_video_to_file, get_random_hashtags
from keyboards.cut_kb import get_cut_mode_keyboard

logger = logging.getLogger(__name__)

router = Router()

URL_PATTERN = re.compile(r'https?://[^\s]+')


def extract_number(filename: str) -> int:
    match = re.search(r'\d+', filename)
    return int(match.group()) if match else 0


def parse_bool_flag(value: Optional[str], default: bool = True) -> bool:
    """
    Parses boolean flag flexibly:
    - yes, y, true, да, д, 1, + -> True
    - no, n, false, нет, н, 0, - -> False
    - None or unrecognized -> default
    """
    if value is None:
        return default
    cleaned = value.strip().lower()
    if cleaned in ("yes", "y", "true", "да", "д", "1", "+"):
        return True
    if cleaned in ("no", "n", "false", "нет", "н", "0", "-"):
        return False
    return default


def parse_cut_args(text: str) -> Tuple[Optional[str], bool]:
    """
    Extracts (url, include_intro) from a command message or text.
    If flag is absent, defaults to True.
    """
    if not text:
        return None, True

    parts = text.strip().split(maxsplit=1)
    target_text = parts[1] if len(parts) > 1 and parts[0].startswith("/") else text

    url_match = URL_PATTERN.search(target_text)
    if not url_match:
        return None, True

    url = url_match.group(0)
    remaining = (target_text[:url_match.start()] + " " + target_text[url_match.end():]).strip()
    flag_tokens = [t for t in remaining.split() if not t.startswith("/")]
    flag_str = flag_tokens[0] if flag_tokens else None
    include_intro = parse_bool_flag(flag_str, default=True)

    return url, include_intro


class CutStates(StatesGroup):
    waiting_for_video = State()


async def run_cut_and_send(
    message: Message,
    status_msg: Message,
    video_path: str,
    zip_path: str,
    extract_dir: str,
    include_intro: bool = True,
):
    """Common pipeline: sends video to backend cut endpoint, extracts zip and uploads clips back to user."""
    try:
        mode_text = "с интро-заголовком" if include_intro else "без интро"
        await status_msg.edit_text(f"Обрабатываем видео ({mode_text})...")

        # 1. Send to backend for cutting
        try:
            await cut_video(video_path, zip_path, include_intro=include_intro)
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

        # 3. Read segments metadata if present
        segments_map = {}
        segments_json_path = os.path.join(extract_dir, "segments.json")
        if os.path.exists(segments_json_path):
            try:
                import json
                with open(segments_json_path, "r", encoding="utf-8") as sf:
                    seg_data = json.load(sf)
                    for seg in seg_data.get("segments", []):
                        segments_map[seg.get("segment_id")] = seg
            except Exception as e:
                logger.warning(f"Failed to parse segments.json: {e}")

        # 4. Send fragments
        try:
            files = [
                f for f in os.listdir(extract_dir)
                if os.path.isfile(os.path.join(extract_dir, f))
                and f.lower().endswith(('.mp4', '.mov', '.mkv', '.webm'))
            ]
            files.sort(key=extract_number)

            if not files:
                await status_msg.edit_text("Сервер вернул пустой архив без фрагментов.")
                return

            await status_msg.edit_text("🎬 Видео сегментировано по смыслу. Отправляю фрагменты...")

            sent_count = 0
            for i, file_name in enumerate(files, 1):
                part_num = extract_number(file_name) or i
                fragment_path = os.path.join(extract_dir, file_name)

                try:
                    tags = await get_random_hashtags(5)
                    tags_str = " ".join(tags)
                except Exception:
                    tags_str = ""

                seg_meta = segments_map.get(part_num)
                if seg_meta:
                    seg_type = "🪝 Хук" if seg_meta.get("segment_type") == "hook" else f"📖 Часть {part_num}"
                    title = seg_meta.get("title", "")
                    start_t = seg_meta.get("start_timestamp", 0.0)
                    end_t = seg_meta.get("end_timestamp", 0.0)
                    caption_header = f"{seg_type}: {title}\n⏱ {start_t:.1f}с - {end_t:.1f}с"
                else:
                    caption_header = f"Часть {part_num}"

                caption = f"{caption_header}\n\n{tags_str}" if tags_str else caption_header

                try:
                    file_size = os.path.getsize(fragment_path)
                    # Telegram limit for Bot API upload via standard server is 50MB (52,428,800 bytes)
                    if not TELEGRAM_LOCAL_SERVER and file_size >= (49.5 * 1024 * 1024):
                        size_mb = file_size / (1024 * 1024)
                        logger.warning(
                            f"Fragment {part_num} ({file_name}, {size_mb:.1f} MB) exceeds Telegram 50MB limit."
                        )
                        await message.answer(
                            f"⚠️ <b>{caption_header}</b>\nФайл фрагмента слишком большой для отправки в Telegram ({size_mb:.1f} МБ > 50 МБ)."
                        )
                        continue

                    input_file = FSInputFile(fragment_path)
                    await message.answer_video(input_file, caption=caption)
                    sent_count += 1
                except TelegramBadRequest as t_err:
                    logger.error(f"TelegramBadRequest sending fragment {part_num}: {t_err}")
                    await message.answer(
                        f"⚠️ Не удалось отправить {caption_header}: {t_err.message}"
                    )
                except Exception as send_err:
                    logger.error(f"Failed to send video fragment {part_num} to Telegram: {send_err}")
                    await message.answer(
                        f"⚠️ Ошибка при отправке фрагмента {part_num} в Telegram."
                    )

            try:
                await status_msg.delete()
            except Exception:
                pass

            if sent_count == 0 and files:
                await message.answer("❌ Ни один фрагмент не удалось отправить в Telegram.")

        except Exception as e:
            logger.error(f"Failed in fragments send loop: {e}")
            await message.answer("Произошла ошибка при отправке фрагментов в Telegram.")

    except Exception as e:
        logger.error(f"Error during cut pipeline: {e}")
        await status_msg.edit_text("Произошла непредвиденная ошибка при обработке.")

@router.message(Command("reddit_cut", "cut", "cut_reddit"))
async def cmd_reddit_cut(message: Message, state: FSMContext):
    url, include_intro = parse_cut_args(message.text or "")
    if url:
        await state.clear()
        await process_url_cut(message, url, include_intro=include_intro)
        return

    await state.set_state(CutStates.waiting_for_video)
    await state.update_data(include_intro=True)
    await message.answer(
        "🎬 <b>Смысловая нарезка видео (Qwen 3 8B)</b>\n\n"
        "Отправь видеофайл (до 20 МБ) или ссылку на YouTube видео.\n\n"
        "💡 <b>Формат команды:</b>\n"
        "<code>/cut_reddit &lt;ссылка&gt; [yes|no]</code>\n"
        "• <code>yes</code> (по умолчанию) — склеивать вводный хук/вопрос с каждой историей\n"
        "• <code>no</code> — нарезать только истории без вступительного интро\n\n"
        "Или выбери режим нарезки кнопкой перед отправкой ссылки/файла:",
        reply_markup=get_cut_mode_keyboard(),
    )


@router.callback_query(F.data.startswith("cut_mode:"))
async def cb_cut_mode(callback: CallbackQuery, state: FSMContext):
    mode_val = callback.data.split(":", 1)[1]
    include_intro = parse_bool_flag(mode_val, default=True)
    await state.update_data(include_intro=include_intro)
    mode_desc = "<b>с заголовком</b> (yes)" if include_intro else "<b>без заголовка</b> (no)"
    await callback.answer(f"Режим: {'С заголовком' if include_intro else 'Без заголовка'}")
    if callback.message:
        await callback.message.edit_text(
            f"✅ Выбран режим нарезки: {mode_desc}\n\n"
            f"Теперь отправь ссылку на YouTube видео или видеофайл (до 20 МБ).\n"
            f"<i>(При необходимости можно переключить режим ниже)</i>",
            reply_markup=get_cut_mode_keyboard(),
        )


async def process_url_cut(message: Message, url: str, include_intro: bool = True):
    mode_desc = "с заголовком" if include_intro else "без заголовка"
    status_msg = await message.answer(f"📥 Видео скачивается по ссылке ({mode_desc})...")
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

        await run_cut_and_send(message, status_msg, video_path, zip_path, extract_dir, include_intro=include_intro)
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)


@router.message(CutStates.waiting_for_video, F.text)
async def handle_text_url(message: Message, state: FSMContext):
    url, parsed_intro = parse_cut_args(message.text or "")
    if url:
        data = await state.get_data()
        tokens = (message.text or "").strip().split()
        # If user typed additional argument in text (e.g. url no), use it; else use state
        if len(tokens) > 1:
            include_intro = parsed_intro
        else:
            include_intro = data.get("include_intro", True)

        await state.clear()
        await process_url_cut(message, url, include_intro=include_intro)
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

    data = await state.get_data()
    include_intro = data.get("include_intro", True)
    await state.clear()

    # Pre-check file size for Standard Telegram Bot API (limit 20MB)
    if not TELEGRAM_LOCAL_SERVER and video_obj.file_size and video_obj.file_size > 20 * 1024 * 1024:
        size_mb = round(video_obj.file_size / (1024 * 1024), 1)
        await message.answer(
            f"❌ <b>Файл слишком большой ({size_mb} MB).</b>\n\n"
            f"Стандартный сервер Telegram Bot API ограничивает скачивание файлов через ботов до <b>20 MB</b>.\n\n"
            f"💡 <b>Что можно сделать:</b>\n"
            f"1️⃣ Отправить <b>ссылку</b> на YouTube видео (например: <code>/reddit_cut https://youtu.be/...</code>)\n"
            f"2️⃣ Или сжать видеофайл до размера менее 20 MB."
        )
        return

    mode_desc = "с заголовком" if include_intro else "без заголовка"
    status_msg = await message.answer(f"Видео получено ({mode_desc}). Скачиваем...")

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
                    "<code>/reddit_cut https://youtu.be/...</code>"
                )
            else:
                logger.error(f"Failed to download video from Telegram: {e}")
                await status_msg.edit_text(f"Ошибка при скачивании видео из Telegram: {e.message}")
            return
        except Exception as e:
            logger.error(f"Failed to download video from Telegram: {e}")
            await status_msg.edit_text("Ошибка при скачивании видео из Telegram.")
            return

        await run_cut_and_send(message, status_msg, video_path, zip_path, extract_dir, include_intro=include_intro)

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)



