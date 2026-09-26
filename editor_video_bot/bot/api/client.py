import logging
from typing import Any, Dict, List, Optional
import aiohttp

from config import API_BASE, BOT_SECRET

logger = logging.getLogger(__name__)

_session: Optional[aiohttp.ClientSession] = None


async def init_session() -> None:
    global _session
    timeout = aiohttp.ClientTimeout(total=1800, connect=60)
    _session = aiohttp.ClientSession(timeout=timeout)


async def close_session() -> None:
    global _session
    if _session:
        await _session.close()
        _session = None


def _get_headers() -> Dict[str, str]:
    headers = {}
    if BOT_SECRET:
        headers["X-Bot-Secret"] = BOT_SECRET
    return headers


async def cut_video(video_path: str, output_zip_path: str) -> None:
    """Sends video to Rust backend for cutting and saves the ZIP result."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/video/cut"
    headers = _get_headers()

    with open(video_path, "rb") as f:
        form = aiohttp.FormData()
        form.add_field("video", f, filename="video.mp4", content_type="video/mp4")

        async with _session.post(url, data=form, headers=headers) as response:
            if response.status != 200:
                text = await response.text()
                raise Exception(f"Backend returned {response.status}: {text}")

            with open(output_zip_path, "wb") as out_f:
                async for chunk in response.content.iter_chunked(8192):
                    out_f.write(chunk)


async def download_video(youtube_url: str) -> bytes:
    """Issues POST /api/v1/video/download to backend and returns video content bytes."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/video/download"
    headers = _get_headers()
    headers["Content-Type"] = "application/json"
    payload = {"url": youtube_url}

    async with _session.post(url, json=payload, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            raise Exception(f"Backend returned {response.status}: {text}")

        return await response.read()


async def download_video_to_file(youtube_url: str, output_path: str) -> None:
    """Issues POST /api/v1/video/download to backend and streams video directly to output_path."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/video/download"
    headers = _get_headers()
    headers["Content-Type"] = "application/json"
    payload = {"url": youtube_url}

    async with _session.post(url, json=payload, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            raise Exception(f"Backend returned {response.status}: {text}")

        with open(output_path, "wb") as out_f:
            async for chunk in response.content.iter_chunked(8192):
                out_f.write(chunk)



async def get_active_accounts() -> List[Dict[str, Any]]:
    """Retrieves list of active accounts from backend."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/accounts"
    headers = _get_headers()

    async with _session.get(url, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            raise Exception(f"Failed to fetch accounts ({response.status}): {text}")
        return await response.json()


async def add_account(
    name: str,
    cookies_path: str,
    proxy_url: str = "",
    publish_time: str = "13:00:00",
    interval_days: int = 1,
) -> Dict[str, Any]:
    """Adds a new account via backend API."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/accounts"
    headers = _get_headers()
    headers["Content-Type"] = "application/json"
    payload = {
        "name": name,
        "cookies_path": cookies_path,
        "proxy_url": proxy_url,
        "publish_time": publish_time,
        "interval_days": interval_days,
        "is_active": True,
    }

    async with _session.post(url, json=payload, headers=headers) as response:
        if response.status not in (200, 201):
            text = await response.text()
            raise Exception(f"Failed to create account ({response.status}): {text}")
        return await response.json()


async def schedule_clips(account_id: int, job_id: str) -> Dict[str, Any]:
    """Schedules cut clips for publication on backend."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/queue/schedule"
    headers = _get_headers()
    headers["Content-Type"] = "application/json"
    payload = {"account_id": account_id, "job_id": job_id}

    async with _session.post(url, json=payload, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            raise Exception(f"Failed to schedule clips ({response.status}): {text}")
        return await response.json()


async def claim_due_task() -> Optional[Dict[str, Any]]:
    """Polls backend for the next due publish task."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/queue/claim_due"
    headers = _get_headers()

    async with _session.post(url, headers=headers) as response:
        if response.status == 204:
            return None
        if response.status != 200:
            text = await response.text()
            logger.error(f"Error claiming due task ({response.status}): {text}")
            return None
        return await response.json()


async def update_task_status(task_id: int, status: str, error_log: Optional[str] = None) -> None:
    """Updates status of a publish task on backend."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/queue/update_status"
    headers = _get_headers()
    headers["Content-Type"] = "application/json"
    payload = {"task_id": task_id, "status": status, "error_log": error_log}

    async with _session.post(url, json=payload, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            logger.error(f"Failed to update task status ({response.status}): {text}")


async def clear_account_videos(account_id: int) -> Dict[str, Any]:
    """Clears all video archive files and queue for an account on backend."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/accounts/{account_id}/videos"
    headers = _get_headers()

    async with _session.delete(url, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            raise Exception(f"Failed to clear account videos ({response.status}): {text}")
        return await response.json()


async def get_random_hashtags(count: int = 5) -> List[str]:
    """Fetches count random hashtags from backend API."""
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/hashtags/random?count={count}"
    headers = _get_headers()

    async with _session.get(url, headers=headers) as response:
        if response.status != 200:
            text = await response.text()
            logger.error(f"Failed to fetch hashtags ({response.status}): {text}")
            return []
        return await response.json()

