import os
import shutil
import tempfile
import zipfile
import asyncio
from aiogram import Router, F, Bot
from aiogram.filters import Command
from aiogram.types import Message, FSInputFile
from aiogram.fsm.context import FSMContext
from aiogram.fsm.state import StatesGroup, State

import logging
from api.client import cut_video

logger = logging.getLogger(__name__)

router = Router()

class CutStates(StatesGroup):
    waiting_for_video = State()

@router.message(Command("cut"))
async def cmd_cut(message: Message, state: FSMContext):
    await message.answer("Отправь видео, которое нужно разделить на фрагменты.")
    await state.set_state(CutStates.waiting_for_video)

@router.message(CutStates.waiting_for_video, F.video)
async def handle_video(message: Message, state: FSMContext, bot: Bot):
    await state.clear()
    
    video = message.video
    if not video:
        return

    msg = await message.answer("Видео получено. Скачиваем...")

    temp_dir = tempfile.mkdtemp()
    video_path = os.path.join(temp_dir, f"video_{message.message_id}.mp4")
    zip_path = os.path.join(temp_dir, f"result_{message.message_id}.zip")
    extract_dir = os.path.join(temp_dir, "fragments")

    try:
        # 1. Download video
        try:
            await bot.download(video, destination=video_path)
        except Exception as e:
            logger.error(f"Failed to download video from Telegram: {e}")
            await msg.edit_text("Ошибка при скачивании видео из Telegram.")
            return

        await msg.edit_text("Обрабатываем видео...")

        # 2. Send to backend
        try:
            # You can wrap this in asyncio.wait_for if you need strict timeout, 
            # though aiohttp ClientSession can have its own timeout.
            await cut_video(video_path, zip_path)
        except asyncio.TimeoutError:
            logger.error("Backend request timed out.")
            await msg.edit_text("Превышено время ожидания ответа от сервера (таймаут).")
            return
        except Exception as e:
            logger.error(f"Backend processing failed: {e}")
            await msg.edit_text("Ошибка при обработке видео на сервере.")
            return

        # 3. Extract ZIP
        try:
            os.makedirs(extract_dir, exist_ok=True)
            with zipfile.ZipFile(zip_path, 'r') as zip_ref:
                zip_ref.extractall(extract_dir)
        except Exception as e:
            logger.error(f"Failed to extract ZIP: {e}")
            await msg.edit_text("Ошибка при распаковке ответа от сервера.")
            return

        # 4. Send fragments
        try:
            files = [f for f in os.listdir(extract_dir) if os.path.isfile(os.path.join(extract_dir, f))]
            files.sort()
            
            if not files:
                await msg.edit_text("Сервер вернул пустой архив без фрагментов.")
                return

            await msg.edit_text("Видео разделено. Отправляю фрагменты...")
            
            for file_name in files:
                fragment_path = os.path.join(extract_dir, file_name)
                input_file = FSInputFile(fragment_path)
                await message.answer_video(input_file)
                
            await msg.delete()
        except Exception as e:
            logger.error(f"Failed to send video fragments to Telegram: {e}")
            await message.answer("Произошла ошибка при отправке фрагментов в Telegram.")

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)
        
@router.message(CutStates.waiting_for_video)
async def handle_not_video(message: Message, state: FSMContext):
    await state.clear()
    await message.answer("Это не видео. Операция отменена. Отправь /cut чтобы попробовать снова.")
