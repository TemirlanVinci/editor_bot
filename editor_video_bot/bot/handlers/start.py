from aiogram import Router
from aiogram.filters import Command
from aiogram.types import Message

router = Router()

HELP_TEXT = (
    "🤖 <b>Панель управления видео-ботом</b>\n\n"
    "📋 <b>Список всех команд бота:</b>\n\n"
    "✂️ <b>Обработка и публикации:</b>\n"
    "• <code>/acc &lt;ссылка&gt;</code> — Нарезать видео и запланировать публикации в TikTok\n"
    "• <code>/cut</code> — Нарезать видеофайлы (до 20 МБ) или видео по ссылке\n"
    "• <code>/download &lt;ссылка&gt;</code> — Скачать видео с YouTube\n\n"
    "📱 <b>Управление аккаунтами TikTok:</b>\n"
    "• <code>/accounts</code> — Список всех активных аккаунтов\n"
    "• <code>/add_account</code> — Добавить новый аккаунт TikTok\n"
    "• <code>/clear_archive</code> — Удалить все ролики из архива выбранного аккаунта\n\n"
    "ℹ️ <b>Справка:</b>\n"
    "• <code>/help</code> — Список команд и инструкции\n"
    "• <code>/start</code> — Главное меню"
)


@router.message(Command("start", "help"))
async def cmd_start_help(message: Message):
    await message.answer(HELP_TEXT, parse_mode="HTML")

