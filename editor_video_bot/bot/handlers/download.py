import asyncio
import logging
from aiogram import Router
from aiogram.exceptions import TelegramBadRequest
from aiogram.filters import Command
from aiogram.types import BufferedInputFile, Message

from api.client import download_video, get_random_hashtags

logger = logging.getLogger(__name__)

router = Router()


@router.message(Command("download"))
async def cmd_download(message: Message):
    args = message.text.split(maxsplit=1) if message.text else []
    if len(args) < 2 or not args[1].strip():
        await message.answer(
            "Пожалуйста, укажите ссылку на YouTube видео.\nПример: `/download https://www.youtube.com/watch?v=...`",
            parse_mode="Markdown",
        )
        return

    url = args[1].strip()
    status_msg = await message.answer("Downloading video, please wait...")

    try:
        video_bytes = await download_video(url)

        # Telegram limit for Bot API file upload via BufferedInputFile is 50MB
        if len(video_bytes) > 52428800:
            await status_msg.edit_text(
                "Скачанный видеофайл превышает лимит отправки Telegram (50 МБ)."
            )
            return

        try:
            tags = await get_random_hashtags(5)
            tags_str = " ".join(tags)
        except Exception:
            tags_str = ""

        buffered_file = BufferedInputFile(video_bytes, filename="video.mp4")
        await message.answer_video(video=buffered_file, caption=tags_str if tags_str else None)
        await status_msg.delete()

    except asyncio.TimeoutError:
        logger.error("Download timed out")
        await status_msg.edit_text("Превышено время ожидания ответа от сервера (таймаут).")
    except TelegramBadRequest as e:
        logger.error(f"Telegram upload failed: {e}")
        await status_msg.edit_text(f"Не удалось отправить видео в Telegram: {e.message}")
    except Exception as e:
        logger.error(f"Download video failed: {e}")
        err_str = str(e)
        if (
            "400" in err_str
            or "validation" in err_str.lower()
            or "invalid" in err_str.lower()
        ):
            await status_msg.edit_text(
                "Невалидная или неподдерживаемая ссылка на YouTube. "
                "Убедитесь, что ссылка ведет на видео или YouTube Shorts."
            )
        else:
            await status_msg.edit_text("Произошла ошибка при скачивании видео на сервере.")
