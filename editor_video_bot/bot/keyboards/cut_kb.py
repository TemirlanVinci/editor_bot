from aiogram.types import InlineKeyboardButton, InlineKeyboardMarkup


def get_cut_mode_keyboard() -> InlineKeyboardMarkup:
    """
    Returns inline keyboard for selecting cutting mode:
    - With intro question/hook (yes)
    - Without intro question/hook (no)
    """
    buttons = [
        [
            InlineKeyboardButton(
                text="✂️ С заголовком (yes)",
                callback_data="cut_mode:yes",
            ),
            InlineKeyboardButton(
                text="🎬 Без заголовка (no)",
                callback_data="cut_mode:no",
            ),
        ]
    ]
    return InlineKeyboardMarkup(inline_keyboard=buttons)
