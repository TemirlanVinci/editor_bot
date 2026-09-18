import os
import re
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

def extract_number(filename: str) -> int:
    match = re.search(r'\d+', filename)
    return int(match.group()) if match else 0

class CutStates(StatesGroup):
    waiting_for_video = State()

@router.message(Command("cut"))
async def cmd_cut(message: Message, state: FSMContext):
    await message.answer("Отправь видео, которое нужно разделить на фрагменты.")
    await state.set_state(CutStates.waiting_for_video)

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
                await message.answer("Этот документ не является видео. Операция отменена. Отправь /cut чтобы попробовать снова.")
            return

    if not video_obj:
        return

    await state.clear()
    
    msg = await message.answer("Видео получено. Скачиваем...")

    temp_dir = tempfile.mkdtemp()
    video_path = os.path.join(temp_dir, f"video_{message.message_id}.mp4")
    zip_path = os.path.join(temp_dir, f"result_{message.message_id}.zip")
    extract_dir = os.path.join(temp_dir, "fragments")

    try:
        # 1. Download video
        try:
            await bot.download(video_obj, destination=video_path)
        except Exception as e:
            logger.error(f"Failed to download video from Telegram: {e}")
            await msg.edit_text("Ошибка при скачивании видео из Telegram.")
            return

        await msg.edit_text("Обрабатываем видео...")

        # 2. Send to backend
        try:
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
            files.sort(key=extract_number)
            
            if not files:
                await msg.edit_text("Сервер вернул пустой архив без фрагментов.")
                return

            await msg.edit_text("Видео разделено. Отправляю фрагменты...")
            
            for i, file_name in enumerate(files, 1):
                part_num = extract_number(file_name) or i
                fragment_path = os.path.join(extract_dir, file_name)
                input_file = FSInputFile(fragment_path)
                await message.answer_video(input_file, caption=str(part_num))
                
            await msg.delete()
        except Exception as e:
            logger.error(f"Failed to send video fragments to Telegram: {e}")
            await message.answer("Произошла ошибка при отправке фрагментов в Telegram.")

    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)
        
@router.message(CutStates.waiting_for_video)
async def handle_not_video(message: Message, state: FSMContext):
    await state.clear()
    await message.answer("Это не видео. Операция отменена. Отправь /cut или пришли видеофайл.")

