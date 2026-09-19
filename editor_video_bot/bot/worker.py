import asyncio
import json
import logging
import os
from typing import Any, Dict, List, Optional
from urllib.parse import urlparse

from api.client import claim_due_task, update_task_status
from config import WORKER_POLL_INTERVAL

logger = logging.getLogger(__name__)


def parse_proxy_url(proxy_url: str) -> Optional[Dict[str, str]]:
    """Parses proxy string into Playwright proxy dictionary format."""
    if not proxy_url or not proxy_url.strip():
        return None

    url = proxy_url.strip()
    if not url.startswith("http://") and not url.startswith("https://") and not url.startswith("socks5://"):
        url = "http://" + url

    parsed = urlparse(url)
    proxy_dict = {"server": f"{parsed.scheme}://{parsed.hostname}:{parsed.port}" if parsed.port else f"{parsed.scheme}://{parsed.hostname}"}

    if parsed.username:
        proxy_dict["username"] = parsed.username
    if parsed.password:
        proxy_dict["password"] = parsed.password

    return proxy_dict


def load_cookies_from_file(cookies_path: str) -> List[Dict[str, Any]]:
    """Loads cookies from a JSON file."""
    if not cookies_path or not os.path.exists(cookies_path):
        logger.warning(f"Cookies file not found: {cookies_path}")
        return []

    try:
        with open(cookies_path, "r", encoding="utf-8") as f:
            cookies = json.load(f)
            if isinstance(cookies, list):
                # Ensure standard keys for Playwright
                cleaned = []
                for c in cookies:
                    if "name" in c and "value" in c:
                        item = {
                            "name": str(c["name"]),
                            "value": str(c["value"]),
                            "domain": c.get("domain", ".tiktok.com"),
                            "path": c.get("path", "/"),
                        }
                        if "sameSite" in c and c["sameSite"] in ("Strict", "Lax", "None"):
                            item["sameSite"] = c["sameSite"]
                        cleaned.append(item)
                return cleaned
    except Exception as e:
        logger.error(f"Error reading cookies file {cookies_path}: {e}")

    return []


async def upload_clip_to_tiktok(task: Dict[str, Any]) -> None:
    """Uses Playwright to upload a video clip to TikTok Studio."""
    from playwright.async_api import async_playwright
    from playwright_stealth import stealth_async

    task_id = task["id"]
    file_path = task["file_path"]
    caption = task["caption"]
    proxy_url = task.get("proxy_url", "")
    cookies_path = task.get("cookies_path", "")

    if not os.path.exists(file_path):
        err_msg = f"Clip file not found on disk: {file_path}"
        logger.error(err_msg)
        await update_task_status(task_id, "failed", error_log=err_msg)
        return

    logger.info(f"🚀 Starting Playwright upload for Task #{task_id} (File: {file_path})...")

    proxy_config = parse_proxy_url(proxy_url)
    cookies = load_cookies_from_file(cookies_path)

    async with async_playwright() as p:
        browser_args = [
            "--no-sandbox",
            "--disable-setuid-sandbox",
            "--disable-blink-features=AutomationControlled",
        ]

        launch_kwargs: Dict[str, Any] = {
            "headless": True,
            "args": browser_args,
        }
        if proxy_config:
            launch_kwargs["proxy"] = proxy_config
            logger.info(f"Using proxy server: {proxy_config['server']}")

        browser = await p.chromium.launch(**launch_kwargs)

        context = await browser.new_context(
            viewport={"width": 1280, "height": 720},
            user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        )

        if cookies:
            await context.add_cookies(cookies)
            logger.info(f"Loaded {len(cookies)} session cookies for account.")

        page = await context.new_page()
        await stealth_async(page)

        try:
            # 1. Navigate to TikTok Studio upload page
            logger.info("Navigating to TikTok Studio upload page...")
            await page.goto("https://www.tiktok.com/tiktokstudio/upload", wait_until="networkidle", timeout=60000)

            # Check if redirected to login page
            if "login" in page.url:
                raise Exception("Redirected to login page. Session cookies might be expired or invalid.")

            # 2. Upload video file
            logger.info("Locating file upload input...")
            file_input = page.locator("input[type='file']")
            await file_input.wait_for(state="attached", timeout=30000)
            await file_input.set_input_files(file_path)

            logger.info("Video file attached, waiting for upload to process...")

            # 3. Wait for caption area and enter text
            await asyncio.sleep(5)
            caption_locator = page.locator("div[contenteditable='true'], textarea, [class*='caption'], [class*='editor']").first
            await caption_locator.wait_for(state="visible", timeout=60000)

            # Clear existing text and write new caption
            await caption_locator.click()
            await page.keyboard.press("Control+A")
            await page.keyboard.press("Backspace")
            await page.keyboard.type(caption, delay=50)

            logger.info(f"Entered caption: '{caption}'")
            await asyncio.sleep(3)

            # 4. Click Post button
            post_button = page.locator("button:has-text('Post'), button:has-text('Опубликовать'), button:has-text('Publish')").first
            await post_button.wait_for(state="visible", timeout=30000)
            await post_button.click()

            logger.info("Clicked Post button, waiting for confirmation...")
            await asyncio.sleep(10)

            # Mark task as published on backend
            await update_task_status(task_id, "published")
            logger.info(f"✅ Task #{task_id} successfully published to TikTok!")

        except Exception as e:
            err_msg = f"Playwright upload error: {e}"
            logger.error(err_msg, exc_info=True)
            await update_task_status(task_id, "failed", error_log=err_msg)

        finally:
            await context.close()
            await browser.close()


async def start_worker_loop() -> None:
    """Main continuous polling loop for TikTok background publisher worker."""
    logger.info(f"🤖 Starting TikTok Cron Worker (Poll interval: {WORKER_POLL_INTERVAL}s)...")

    while True:
        try:
            task = await claim_due_task()
            if task:
                logger.info(f"Claimed due task #{task['id']} for Account #{task['account_id']}")
                await upload_clip_to_tiktok(task)
            else:
                await asyncio.sleep(WORKER_POLL_INTERVAL)
        except asyncio.CancelledError:
            logger.info("Worker loop cancelled.")
            break
        except Exception as e:
            logger.error(f"Error in worker loop: {e}", exc_info=True)
            await asyncio.sleep(WORKER_POLL_INTERVAL)
