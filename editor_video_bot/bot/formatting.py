from datetime import datetime


def format_datetime(iso_string: str) -> str:
    try:
        dt = datetime.fromisoformat(iso_string.replace("Z", "+00:00"))
        return dt.strftime("%d.%m.%Y %H:%M")
    except (ValueError, AttributeError, TypeError):
        return iso_string