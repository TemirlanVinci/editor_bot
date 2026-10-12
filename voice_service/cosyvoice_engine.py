import os
import glob
import logging
import torch
import torchaudio
from typing import Optional

logger = logging.getLogger("voice_service.cosyvoice")

AUDIO_EXTENSIONS = (".wav", ".mp3", ".m4a", ".ogg", ".flac")

class CosyVoiceEngine:
    """
    CosyVoice 3 Voice Conversion Engine.
    Performs speech-to-speech Voice Conversion (inference_vc) matching
    source speech timing, pauses and prosody to a target reference speaker timbre.
    """
    def __init__(self, model_dir: str = "/app/models/CosyVoice-300M", voices_dir: str = "/app/media/voices"):
        self.model_dir = model_dir
        self.voices_dir = voices_dir
        self.model = None
        self._init_attempted = False

    def is_available(self) -> bool:
        """Returns True if CosyVoice model is loaded and ready."""
        if not self._init_attempted:
            self._try_load_model()
        return self.model is not None

    def _try_load_model(self):
        self._init_attempted = True
        if not os.path.exists(self.model_dir):
            logger.warning(f"⚠️ CosyVoice model directory not found at {self.model_dir}. Running in fallback mode.")
            return

        try:
            logger.info(f"⏳ Loading CosyVoice model from {self.model_dir}...")
            # Try importing CosyVoice CLI AutoModel
            from cosyvoice.cli.cosyvoice import AutoModel
            self.model = AutoModel(model_dir=self.model_dir)
            logger.info("✅ CosyVoice model successfully loaded!")
        except ImportError as e:
            logger.warning(f"⚠️ cosyvoice package not installed or import error: {e}. Running in fallback mode.")
        except Exception as e:
            logger.error(f"❌ Failed to load CosyVoice model from {self.model_dir}: {e}")

    def find_voice_sample(self, voice_key: str) -> Optional[str]:
        """
        Locates a reference voice audio sample for the given voice_key:
        1. media/voices/<voice_key>/*.(wav|mp3|m4a|ogg|flac)
        2. media/voices/<voice_key>.(wav|mp3|m4a|ogg|flac)
        """
        if not os.path.exists(self.voices_dir):
            return None

        # 1. Check directory: media/voices/{voice_key}/
        sub_dir = os.path.join(self.voices_dir, voice_key)
        if os.path.isdir(sub_dir):
            for file in os.listdir(sub_dir):
                if file.lower().endswith(AUDIO_EXTENSIONS):
                    full_path = os.path.join(sub_dir, file)
                    if os.path.getsize(full_path) > 0:
                        return full_path

        # 2. Check direct file: media/voices/{voice_key}.*
        for ext in AUDIO_EXTENSIONS:
            direct_file = os.path.join(self.voices_dir, f"{voice_key}{ext}")
            if os.path.isfile(direct_file) and os.path.getsize(direct_file) > 0:
                return direct_file

        return None

    def convert_voice(self, source_audio_path: str, prompt_audio_path: str, output_path: str) -> str:
        """
        Runs Voice Conversion (inference_vc) using CosyVoice.
        """
        if not self.is_available():
            raise RuntimeError("CosyVoice model is not loaded or unavailable")

        if not os.path.exists(source_audio_path):
            raise FileNotFoundError(f"Source audio file not found: {source_audio_path}")
        if not os.path.exists(prompt_audio_path):
            raise FileNotFoundError(f"Prompt reference voice not found: {prompt_audio_path}")

        os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)

        logger.info(
            f"🎙️ Running CosyVoice Voice Conversion: source={source_audio_path}, prompt={prompt_audio_path}..."
        )

        audio_chunks = []
        # inference_vc yields dictionary chunks containing 'tts_speech'
        for item in self.model.inference_vc(source_audio_path, prompt_audio_path):
            speech_chunk = item['tts_speech']
            if isinstance(speech_chunk, torch.Tensor):
                audio_chunks.append(speech_chunk.cpu())

        if not audio_chunks:
            raise RuntimeError("CosyVoice inference_vc produced no output chunks")

        final_speech = torch.cat(audio_chunks, dim=-1)
        torchaudio.save(output_path, final_speech, self.model.sample_rate)
        logger.info(f"✅ CosyVoice conversion completed: {output_path}")
        return output_path
