from aiogram import Router, F
from aiogram.filters import Command
from aiogram.types import Message
from keyboards.acc_kb import get_main_reply_keyboard, get_start_inline_keyboard

router = Router()

HELP_TEXT = (
    "🤖 <b>Панель управления видео-ботом</b>\n\n"
    "Выберите нужное действие кнопками ниже без ручного ввода команд:\n\n"
    "📱 <b>Аккаунты и директории TikTok:</b>\n"
    "• Нажмите <b>📁 Видео по аккаунтам</b> — бот загрузит аккаунты из базы и покажет кнопки с количеством видео в каждом\n"
    "• Нажмите <b>📊 Сводка всех видео</b> — суммарная статистика по всем папкам\n"
    "• Нажмите <b>📱 Список аккаунтов</b> — список всех активных аккаунтов TikTok\n\n"
    "✂️ <b>Смысловая нарезка (Whisper + Qwen 3.5 9B):</b>\n"
    "• <code>/reddit_acc &lt;ссылка&gt; [yes|no]</code> — Нарезать видео по смыслу и запланировать в TikTok\n"
    "• <code>/reddit_cut &lt;ссылка&gt; [yes|no]</code> — Нарезать видео на клипы\n"
    "• <code>/download &lt;ссылка&gt;</code> — Скачать видео с YouTube\n\n"
    "⚙️ <b>Управление:</b>\n"
    "• <code>/add_account</code> — Добавить новый аккаунт TikTok\n"
    "• <code>/clear_archive</code> — Очистить архив видео аккаунта\n"
)


@router.message(Command("start", "help"))
@router.message(F.text == "ℹ️ Помощь / Меню")
async def cmd_start_help(message: Message):
    await message.answer(
        HELP_TEXT,
        parse_mode="HTML",
        reply_markup=get_start_inline_keyboard(),
    )
    # Also ensure persistent bottom keyboard is set
    await message.answer(
        "👇 Кнопки быстрого доступа активны внизу экрана:",
        reply_markup=get_main_reply_keyboard(),
    )

