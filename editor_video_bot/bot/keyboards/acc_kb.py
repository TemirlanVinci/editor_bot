from aiogram.types import InlineKeyboardButton, InlineKeyboardMarkup
from typing import List, Dict, Any


def get_account_selection_keyboard(accounts: List[Dict[str, Any]], job_id: str) -> InlineKeyboardMarkup:
    """
    Builds Inline Keyboard for selecting an active TikTok account or cancelling.
    Callback data formats:
    - Account: pub:<account_id>:<job_id>
    - Cancel: pub:cancel:<job_id>
    """
    buttons = []
    
    for acc in accounts:
        btn_text = f"📱 {acc['name']}"
        cb_data = f"pub:{acc['id']}:{job_id}"
        buttons.append([InlineKeyboardButton(text=btn_text, callback_data=cb_data)])

    # Cancel button
    buttons.append([
        InlineKeyboardButton(
            text="❌ Cancel / Do not upload",
            callback_data=f"pub:cancel:{job_id}"
        )
    ])

    return InlineKeyboardMarkup(inline_keyboard=buttons)


def get_clear_archive_keyboard(accounts: List[Dict[str, Any]]) -> InlineKeyboardMarkup:
    """
    Builds Inline Keyboard for selecting an account to clear archive videos.
    """
    buttons = []
    for acc in accounts:
        btn_text = f"📱 #{acc['id']} {acc['name']}"
        cb_data = f"clear_acc:select:{acc['id']}"
        buttons.append([InlineKeyboardButton(text=btn_text, callback_data=cb_data)])

    buttons.append([
        InlineKeyboardButton(
            text="❌ Отмена",
            callback_data="clear_acc:cancel"
        )
    ])
    return InlineKeyboardMarkup(inline_keyboard=buttons)


def get_confirm_clear_keyboard(account_id: int) -> InlineKeyboardMarkup:
    """
    Builds Inline Keyboard for confirming deletion of archive videos.
    """
    buttons = [
        [
            InlineKeyboardButton(
                text="🗑 Да, удалить все видео",
                callback_data=f"clear_acc:confirm:{account_id}"
            )
        ],
        [
            InlineKeyboardButton(
                text="❌ Отмена",
                callback_data="clear_acc:cancel"
            )
        ]
    ]
    return InlineKeyboardMarkup(inline_keyboard=buttons)

