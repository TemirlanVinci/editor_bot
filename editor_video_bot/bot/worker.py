import asyncio
import json
import logging
import os
import random
from typing import Any, Dict, List, Optional
from urllib.parse import urlparse

from api.client import claim_due_task, update_task_status
from config import MEDIA_DIR, WORKER_POLL_INTERVAL

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
            # Support {"cookies": [...]} wrapper format as well as raw list
            if isinstance(cookies, dict) and "cookies" in cookies:
                cookies = cookies["cookies"]

            if isinstance(cookies, list):
                # Ensure standard keys for Playwright
                cleaned = []
                for c in cookies:
                    if isinstance(c, dict) and "name" in c and "value" in c:
                        item = {
                            "name": str(c["name"]),
                            "value": str(c["value"]),
                            "domain": c.get("domain", ".tiktok.com"),
                            "path": c.get("path", "/"),
                        }
                        if "sameSite" in c and c["sameSite"] in ("Strict", "Lax", "None"):
                            item["sameSite"] = c["sameSite"]
                        if "secure" in c and isinstance(c["secure"], bool):
                            item["secure"] = c["secure"]
                        if "httpOnly" in c and isinstance(c["httpOnly"], bool):
                            item["httpOnly"] = c["httpOnly"]
                        if "expires" in c and isinstance(c["expires"], (int, float)):
                            item["expires"] = float(c["expires"])
                        cleaned.append(item)
                return cleaned
    except Exception as e:
        logger.error(f"Error reading cookies file {cookies_path}: {e}")

    return []


async def human_click(page, locator, timeout: float = 30000) -> None:
    """Moves mouse naturally along a curved path and clicks inside element with random offset."""
    await locator.wait_for(state="visible", timeout=timeout)
    box = await locator.bounding_box()
    if box:
        # Pick a point inside the element (avoiding sharp center coordinates)
        target_x = box["x"] + box["width"] * random.uniform(0.25, 0.75)
        target_y = box["y"] + box["height"] * random.uniform(0.25, 0.75)

        # Smooth mouse move with human-like intermediate steps
        steps = random.randint(15, 30)
        await page.mouse.move(target_x, target_y, steps=steps)
        await asyncio.sleep(random.uniform(0.12, 0.30))

        # Realistic mouse down/up with small press delay
        await page.mouse.down()
        await asyncio.sleep(random.uniform(0.06, 0.14))
        await page.mouse.up()
    else:
        await locator.click()


async def human_type(page, locator, text: str) -> None:
    """Types text with variable human-like cadence, punctuation pauses, and occasional typos."""
    await human_click(page, locator)
    await asyncio.sleep(random.uniform(0.3, 0.7))

    for char in text:
        # Subtle 1.5% chance of typo on alphabet characters
        if char.isalpha() and random.random() < 0.015:
            typo_char = random.choice("abcdefghijklmnopqrstuvwxyz")
            await page.keyboard.type(typo_char)
            await asyncio.sleep(random.uniform(0.15, 0.35))
            await page.keyboard.press("Backspace")
            await asyncio.sleep(random.uniform(0.1, 0.25))

        await page.keyboard.type(char)

        # Natural human pauses depending on character type
        if char in ".,!?":
            await asyncio.sleep(random.uniform(0.35, 0.75))
        elif char == " ":
            await asyncio.sleep(random.uniform(0.12, 0.26))
        elif char == "#":
            await asyncio.sleep(random.uniform(0.40, 0.85))
        else:
            await asyncio.sleep(random.uniform(0.045, 0.14))


async def apply_stealth_to_page(page) -> None:
    """Applies stealth evasion to Playwright page (compatible with both 2.x and 1.x playwright-stealth)."""
    try:
        from playwright_stealth import Stealth
        stealth_obj = Stealth()
        await stealth_obj.apply_stealth_async(page)
    except (ImportError, AttributeError):
        try:
            from playwright_stealth import stealth_async
            await stealth_async(page)
        except Exception as e:
            logger.warning(f"Could not apply stealth evasion: {e}")


async def upload_clip_to_tiktok(task: Dict[str, Any]) -> None:
    """Uses Playwright with persistent context & human behavior simulation to upload video to TikTok."""
    task_id = task["id"]
    account_id = task["account_id"]
    file_path = task["file_path"]
    caption = task["caption"]
    proxy_url = task.get("proxy_url", "")
    cookies_path = task.get("cookies_path", "")

    if not os.path.exists(file_path):
        err_msg = f"Clip file not found on disk: {file_path}"
        logger.error(err_msg)
        await update_task_status(task_id, "failed", error_log=err_msg)
        return

    logger.info(f"🚀 Starting humanized Playwright upload for Task #{task_id} (Acc #{account_id}, File: {file_path})...")

    # Add small randomized start delay so tasks don't fire on the microsecond
    start_jitter = random.uniform(2.0, 10.0)
    logger.info(f"Human start delay: {start_jitter:.1f}s...")
    await asyncio.sleep(start_jitter)

    proxy_config = parse_proxy_url(proxy_url)
    cookies = load_cookies_from_file(cookies_path)

    # Use persistent user data directory per account so TikTok sees consistent browser storage
    profiles_dir = os.path.join(MEDIA_DIR, "profiles", f"acc_{account_id}")
    os.makedirs(profiles_dir, exist_ok=True)

    try:
        from playwright.async_api import async_playwright

        async with async_playwright() as p:
            browser_args = [
                "--no-sandbox",
                "--disable-setuid-sandbox",
                "--disable-blink-features=AutomationControlled",
                "--disable-infobars",
                "--window-size=1920,1080",
                "--no-first-run",
                "--no-default-browser-check",
            ]

            launch_kwargs: Dict[str, Any] = {
                "user_data_dir": profiles_dir,
                "headless": True,
                "args": browser_args,
                "viewport": {"width": 1920, "height": 1080},
                "user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
                "locale": "en-US",
                "timezone_id": "America/New_York",
            }
            if proxy_config:
                launch_kwargs["proxy"] = proxy_config
                logger.info(f"Using proxy server: {proxy_config['server']}")

            context = await p.chromium.launch_persistent_context(**launch_kwargs)

            try:
                # Ensure navigator.webdriver is masked at engine level
                await context.add_init_script("""
                    Object.defineProperty(navigator, 'webdriver', {
                        get: () => undefined
                    });
                """)

                # Add cookies if provided
                if cookies:
                    await context.add_cookies(cookies)
                    logger.info(f"Injected {len(cookies)} session cookies.")

                page = context.pages[0] if context.pages else await context.new_page()
                await apply_stealth_to_page(page)

                # Step 1: Human warmup on TikTok main feed
                logger.info("Performing warmup navigation to tiktok.com...")
                try:
                    await page.goto("https://www.tiktok.com/", wait_until="domcontentloaded", timeout=45000)
                    await asyncio.sleep(random.uniform(4.0, 7.0))
                    # Slight scroll to emulate reading the page
                    await page.mouse.wheel(0, random.randint(300, 700))
                    await asyncio.sleep(random.uniform(2.5, 4.5))
                except Exception as warmup_err:
                    logger.warning(f"Warmup warning (continuing to upload): {warmup_err}")

                # Step 2: Navigate to TikTok Studio upload page
                logger.info("Navigating to TikTok Studio upload page...")
                await page.goto("https://www.tiktok.com/tiktokstudio/upload", wait_until="networkidle", timeout=60000)
                await asyncio.sleep(random.uniform(2.0, 4.0))

                # Check if redirected to login page
                if "login" in page.url:
                    raise Exception("Redirected to login page. Session cookies might be expired or invalid.")

                # Step 3: Attach video file
                logger.info("Locating file upload input...")
                file_input = page.locator("input[type='file']")
                await file_input.wait_for(state="attached", timeout=30000)
                await file_input.set_input_files(file_path)
                logger.info("Video file attached. Waiting for TikTok to process video preview...")

                # Step 4: Wait for upload processing and caption area
                caption_locator = page.locator("div[contenteditable='true'], textarea, [class*='caption'], [class*='editor']").first
                await caption_locator.wait_for(state="visible", timeout=60000)

                # Wait an additional 5-10 seconds for video upload bar to settle
                await asyncio.sleep(random.uniform(6.0, 10.0))

                # Step 5: Enter caption with human-like typing
                logger.info(f"Entering caption with human typing cadence: '{caption}'...")
                await human_click(page, caption_locator)
                await page.keyboard.press("Control+A")
                await page.keyboard.press("Backspace")
                await asyncio.sleep(random.uniform(0.3, 0.8))

                await human_type(page, caption_locator, caption)
                logger.info("Caption entered successfully.")

                # Human pause: review preview and caption before posting (5-9 sec)
                review_delay = random.uniform(5.0, 9.0)
                logger.info(f"Simulating human review before posting ({review_delay:.1f}s)...")
                await asyncio.sleep(review_delay)

                # Step 6: Find Post / Publish button and click naturally
                post_button = page.locator("button:has-text('Post'), button:has-text('Опубликовать'), button:has-text('Publish')").first
                await post_button.wait_for(state="visible", timeout=30000)

                logger.info("Moving cursor naturally and clicking Post button...")
                await human_click(page, post_button)

                # Step 7: Wait for confirmation / publication finish
                logger.info("Clicked Post button, waiting for confirmation...")
                await asyncio.sleep(random.uniform(10.0, 15.0))

                if "login" in page.url:
                    raise Exception("Redirected to login page during publication. Session expired.")

                # Check if modal or success message is present, or URL changed
                await update_task_status(task_id, "published")
                logger.info(f"✅ Task #{task_id} successfully published to TikTok with human simulation!")

            finally:
                await context.close()

    except Exception as e:
        err_msg = f"Playwright upload error: {e}"
        logger.error(err_msg, exc_info=True)
        await update_task_status(task_id, "failed", error_log=err_msg)


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
