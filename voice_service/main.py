import os
import shutil
import logging
from typing import Optional, List
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

from demucs_engine import DemucsEngine
from cosyvoice_engine import CosyVoiceEngine

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s"
)
logger = logging.getLogger("voice_service")

PORT = int(os.environ.get("PORT", 8082))
MEDIA_DIR = os.environ.get("MEDIA_DIR", "/app/media")
VOICES_DIR = os.environ.get("VOICES_DIR", os.path.join(MEDIA_DIR, "voices"))
DEMUCS_DEVICE = os.environ.get("DEMUCS_DEVICE", "cuda")
COSYVOICE_MODEL_DIR = os.environ.get("COSYVOICE_MODEL_DIR", "/app/models/CosyVoice-300M")

app = FastAPI(title="Voice Processing Service (Demucs v4 + CosyVoice 3)")

demucs_engine = DemucsEngine(device=DEMUCS_DEVICE)
cosyvoice_engine = CosyVoiceEngine(model_dir=COSYVOICE_MODEL_DIR, voices_dir=VOICES_DIR)

@app.on_event("startup")
def startup_event():
    logger.info(f"🚀 Initializing Voice Service (Demucs device: {demucs_engine.device})...")
    if cosyvoice_engine.is_available():
        logger.info("✅ CosyVoice engine is ready.")
    else:
        logger.warning("⚠️ CosyVoice engine is in fallback mode.")

class SeparateVocalsRequest(BaseModel):
    source_audio_path: str
    output_audio_path: str

class VoiceConvertRequest(BaseModel):
    source_audio_path: str
    output_audio_path: str
    voice: str
    enable_demucs: bool = True

class VoiceConvertResponse(BaseModel):
    status: str  # "success", "fallback", "error"
    output_path: Optional[str] = None
    voice_applied: Optional[str] = None
    demucs_applied: bool = False
    message: Optional[str] = None

@app.get("/health")
def health_check():
    voice_keys = ["male_adult", "male_young", "female_adult", "female_young"]
    available_voices = {}
    for key in voice_keys:
        sample = cosyvoice_engine.find_voice_sample(key)
        available_voices[key] = sample is not None

    return {
        "status": "ok",
        "cosyvoice_loaded": cosyvoice_engine.is_available(),
        "demucs_device": demucs_engine.device,
        "voices_dir": VOICES_DIR,
        "available_voices": available_voices,
    }

@app.post("/api/v1/voice/separate_vocals")
def separate_vocals(req: SeparateVocalsRequest):
    try:
        out = demucs_engine.separate_vocals(req.source_audio_path, req.output_audio_path)
        return {"status": "ok", "output_path": out}
    except Exception as e:
        logger.error(f"Demucs separation error: {e}")
        raise HTTPException(status_code=500, detail=str(e))

@app.post("/api/v1/voice/convert", response_model=VoiceConvertResponse)
def convert_voice(req: VoiceConvertRequest):
    logger.info(
        f"📥 Received convert_voice request: voice='{req.voice}', demucs={req.enable_demucs}, source={req.source_audio_path}"
    )

    if not os.path.exists(req.source_audio_path):
        raise HTTPException(
            status_code=400, detail=f"Source audio file not found: {req.source_audio_path}"
        )

    # 1. Check if reference voice sample exists in media/voices/<voice>/
    prompt_sample = cosyvoice_engine.find_voice_sample(req.voice)
    demucs_done = False
    intermediate_audio = req.source_audio_path

    # Step A: Optional Demucs isolation
    if req.enable_demucs:
        vocals_path = req.output_audio_path + ".vocals.wav"
        try:
            intermediate_audio = demucs_engine.separate_vocals(
                req.source_audio_path, vocals_path
            )
            demucs_done = True
        except Exception as e:
            logger.warning(
                f"⚠️ Demucs separation failed ({e}). Proceeding with original audio."
            )
            intermediate_audio = req.source_audio_path

    # Step B: Voice Conversion (CosyVoice 3)
    if prompt_sample and cosyvoice_engine.is_available():
        try:
            final_out = cosyvoice_engine.convert_voice(
                source_audio_path=intermediate_audio,
                prompt_audio_path=prompt_sample,
                output_path=req.output_audio_path,
            )
            # Cleanup intermediate vocals if separate from output
            if demucs_done and intermediate_audio != req.output_audio_path and os.path.exists(intermediate_audio):
                try:
                    os.remove(intermediate_audio)
                except OSError:
                    pass

            return VoiceConvertResponse(
                status="success",
                output_path=final_out,
                voice_applied=req.voice,
                demucs_applied=demucs_done,
                message=f"Voice converted to '{req.voice}' successfully.",
            )
        except Exception as e:
            logger.error(f"❌ CosyVoice conversion failed: {e}. Falling back to clean audio.")
            # Fallback: copy intermediate audio to output_path
            os.makedirs(os.path.dirname(os.path.abspath(req.output_audio_path)), exist_ok=True)
            shutil.copyfile(intermediate_audio, req.output_audio_path)
            return VoiceConvertResponse(
                status="fallback",
                output_path=req.output_audio_path,
                voice_applied=None,
                demucs_applied=demucs_done,
                message=f"CosyVoice conversion failed ({e}), kept original clean vocals.",
            )
    else:
        # Prompt sample not found or model not loaded -> Fallback gracefully
        reason = "Prompt voice not found in media/voices" if not prompt_sample else "CosyVoice model not ready"
        logger.info(
            f"ℹ️ {reason} for '{req.voice}'. Returning clean audio without voice change."
        )
        if intermediate_audio != req.output_audio_path:
            os.makedirs(os.path.dirname(os.path.abspath(req.output_audio_path)), exist_ok=True)
            shutil.copyfile(intermediate_audio, req.output_audio_path)
            if demucs_done and os.path.exists(intermediate_audio):
                try:
                    os.remove(intermediate_audio)
                except OSError:
                    pass

        return VoiceConvertResponse(
            status="fallback",
            output_path=req.output_audio_path,
            voice_applied=None,
            demucs_applied=demucs_done,
            message=f"{reason}. Original audio retained.",
        )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run("main:app", host="0.0.0.0", port=PORT)
