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
