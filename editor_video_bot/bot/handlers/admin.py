from aiogram import Router
from aiogram.filters import Command
from aiogram.types import Message

router = Router()

@router.message(Command("admin"))
async def cmd_admin(message: Message):
    pass
