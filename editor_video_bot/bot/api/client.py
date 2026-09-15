import aiohttp
from config import API_BASE, BOT_SECRET

_session: aiohttp.ClientSession | None = None

async def init_session():
    global _session
    timeout = aiohttp.ClientTimeout(total=1800, connect=60)
    _session = aiohttp.ClientSession(timeout=timeout)

async def close_session():
    global _session
    if _session:
        await _session.close()
        _session = None

async def cut_video(video_path: str, output_zip_path: str):
    """
    Sends video to Rust backend for cutting and saves the ZIP result.
    Raises Exception if backend response is not 200.
    """
    global _session
    if _session is None:
        await init_session()

    url = f"{API_BASE.rstrip('/')}/api/v1/video/cut"
    headers = {"X-Bot-Secret": BOT_SECRET}
    
    with open(video_path, 'rb') as f:
        form = aiohttp.FormData()
        form.add_field('video', f, filename='video.mp4', content_type='video/mp4')
        
        async with _session.post(url, data=form, headers=headers) as response:
            if response.status != 200:
                text = await response.text()
                raise Exception(f"Backend returned {response.status}: {text}")
            
            with open(output_zip_path, 'wb') as out_f:
                async for chunk in response.content.iter_chunked(8192):
                    out_f.write(chunk)
