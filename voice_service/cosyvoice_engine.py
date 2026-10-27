import os
import glob
import logging
import math
import tempfile
import numpy as np
import torch
import torchaudio
from typing import Optional, List, Tuple

# Compatibility fix for diffusers / cosyvoice with newer huggingface_hub
try:
    import huggingface_hub
    if not hasattr(huggingface_hub, 'cached_download'):
        huggingface_hub.cached_download = huggingface_hub.hf_hub_download
except Exception:
    pass

# Compatibility fix for torchaudio.load avoiding torchcodec dependency
try:
    import soundfile as sf
    def _safe_torchaudio_load(filepath, *args, **kwargs):
        data, sr = sf.read(filepath, dtype='float32')
        tensor = torch.from_numpy(data)
        if tensor.ndim == 1:
            tensor = tensor.unsqueeze(0)
        else:
            tensor = tensor.t()
        return tensor, sr
    torchaudio.load = _safe_torchaudio_load
except Exception:
    pass

logger = logging.getLogger("voice_service.cosyvoice")

AUDIO_EXTENSIONS = (".wav", ".mp3", ".m4a", ".ogg", ".flac")

def _smooth_fade(tensor: torch.Tensor, sr: int = 24000, fade_ms: int = 5) -> torch.Tensor:
    """Applies a smooth micro-fade (in/out) to eliminate boundary pops/clicks."""
    fade_len = int(fade_ms * sr / 1000)
    if tensor.shape[-1] > 2 * fade_len and fade_len > 0:
        fade_in = torch.linspace(0.0, 1.0, fade_len, dtype=tensor.dtype, device=tensor.device)
        fade_out = torch.linspace(1.0, 0.0, fade_len, dtype=tensor.dtype, device=tensor.device)
        tensor[..., :fade_len] *= fade_in
        tensor[..., -fade_len:] *= fade_out
    return tensor

def _find_speech_split_points(source_data: np.ndarray, sr: int, max_chunk_sec: float = 24.0) -> List[int]:
    """
    Finds optimal speech split points (sample indices) at natural pauses/silence
    so that every chunk is <= max_chunk_sec (ideal for CosyVoice flow matching).
    """
    total_samples = len(source_data)
    total_dur = total_samples / sr
    if total_dur <= max_chunk_sec:
        return [0, total_samples]

    n_chunks = max(2, int(np.ceil(total_dur / 20.0)))
    mono = source_data.mean(axis=1) if source_data.ndim > 1 else source_data
    frame_len = max(1, int(0.05 * sr))  # 50ms energy window
    kernel = np.ones(frame_len, dtype=np.float32) / frame_len

    splits = [0]
    for k in range(1, n_chunks):
        nominal_sample = int(k * total_samples / n_chunks)
        radius = int(3.5 * sr)
        min_sample = max(splits[-1] + int(8.0 * sr), nominal_sample - radius)
        max_sample = min(splits[-1] + int(max_chunk_sec * sr), nominal_sample + radius)
        max_sample = min(max_sample, total_samples - (n_chunks - k) * int(6.0 * sr))

        if min_sample < max_sample:
            window = mono[min_sample:max_sample]
            if len(window) > frame_len:
                energy = np.convolve(window ** 2, kernel, mode='valid')
                best_offset = int(np.argmin(energy)) + frame_len // 2
                split_pt = min_sample + best_offset
            else:
                split_pt = (min_sample + max_sample) // 2
        else:
            split_pt = nominal_sample
        splits.append(split_pt)

    splits.append(total_samples)
    return splits

class CosyVoiceEngine:
    """
    CosyVoice Voice Conversion Engine.
    Performs speech-to-speech Voice Conversion (inference_vc) matching
    source speech timing, pauses and prosody to a target reference speaker timbre.
    Includes automatic speech-aware chunking and token sliding window for long audios (>30s).
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

    def _patch_speech_token_extractor(self):
        """
        Monkey-patches CosyVoice's _extract_speech_token with a sliding window
        to ensure any audio >30s never triggers 'assert speech <= 30s'.
        """
        if not self.model or not hasattr(self.model, "frontend"):
            return

        frontend = self.model.frontend
        if not hasattr(frontend, "_extract_speech_token"):
            return

        def _safe_extract_speech_token(prompt_wav):
            from cosyvoice.utils.file_utils import load_wav
            import whisper

            speech = load_wav(prompt_wav, 16000)
            # If <= 30s, use native fast path
            if speech.shape[1] / 16000.0 <= 30.0:
                feat = whisper.log_mel_spectrogram(speech, n_mels=128)
                tokens = frontend.speech_tokenizer_session.run(
                    None,
                    {
                        frontend.speech_tokenizer_session.get_inputs()[0].name: feat.detach().cpu().numpy(),
                        frontend.speech_tokenizer_session.get_inputs()[1].name: np.array([feat.shape[2]], dtype=np.int32)
                    }
                )[0].flatten().tolist()
            else:
                # Sliding window chunking (25s chunks) for long audio safety
                max_samples = 25 * 16000
                tokens = []
                for start in range(0, speech.shape[1], max_samples):
                    chunk = speech[:, start : min(start + max_samples, speech.shape[1])]
                    feat = whisper.log_mel_spectrogram(chunk, n_mels=128)
                    chunk_tokens = frontend.speech_tokenizer_session.run(
                        None,
                        {
                            frontend.speech_tokenizer_session.get_inputs()[0].name: feat.detach().cpu().numpy(),
                            frontend.speech_tokenizer_session.get_inputs()[1].name: np.array([feat.shape[2]], dtype=np.int32)
                        }
                    )[0].flatten().tolist()
                    tokens.extend(chunk_tokens)

            token_tensor = torch.tensor([tokens], dtype=torch.int32).to(frontend.device)
            token_len_tensor = torch.tensor([token_tensor.shape[1]], dtype=torch.int32).to(frontend.device)
            return token_tensor, token_len_tensor

        frontend._extract_speech_token = _safe_extract_speech_token
        logger.info("🛡️ Patched CosyVoice speech token extractor with safe sliding window.")

    def _try_load_model(self):
        self._init_attempted = True
        # If directory doesn't exist or is empty, attempt automatic download from ModelScope
        if not os.path.exists(self.model_dir) or not os.listdir(self.model_dir):
            logger.info(f"⬇️ CosyVoice model not found at {self.model_dir}. Downloading from ModelScope (iic/CosyVoice-300M)...")
            try:
                from modelscope import snapshot_download
                os.makedirs(self.model_dir, exist_ok=True)
                snapshot_download('iic/CosyVoice-300M', local_dir=self.model_dir)
                logger.info(f"✅ CosyVoice model successfully downloaded to {self.model_dir}!")
            except Exception as e:
                logger.error(f"❌ Failed to download CosyVoice model from ModelScope: {e}")
                return

        try:
            logger.info(f"⏳ Loading CosyVoice model from {self.model_dir}...")
            # Try importing CosyVoice CLI AutoModel or CosyVoice
            try:
                from cosyvoice.cli.cosyvoice import AutoModel
                self.model = AutoModel(model_dir=self.model_dir)
            except (ImportError, AttributeError):
                from cosyvoice.cli.cosyvoice import CosyVoice
                self.model = CosyVoice(self.model_dir)

            self._patch_speech_token_extractor()
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

    def _convert_single_chunk(self, source_path: str, prompt_path: str) -> torch.Tensor:
        """Inference Voice Conversion on a single chunk (<=24s) yielding speech tensor."""
        audio_chunks = []
        for item in self.model.inference_vc(source_path, prompt_path):
            speech_chunk = item.get('tts_speech')
            if isinstance(speech_chunk, torch.Tensor):
                audio_chunks.append(speech_chunk.cpu())

        if not audio_chunks:
            raise RuntimeError(f"CosyVoice inference_vc produced no output for chunk {source_path}")

        full_speech = torch.cat(audio_chunks, dim=-1)
        return full_speech

    def _save_audio(self, tensor: torch.Tensor, output_path: str):
        """Saves audio tensor to disk using soundfile or torchaudio."""
        import soundfile as sf
        audio_np = tensor.squeeze().numpy()
        try:
            sf.write(output_path, audio_np, self.model.sample_rate)
        except Exception:
            torchaudio.save(output_path, tensor, self.model.sample_rate)

    def convert_voice(self, source_audio_path: str, prompt_audio_path: str, output_path: str) -> str:
        """
        Runs Voice Conversion (inference_vc) using CosyVoice.
        Seamlessly chunks source audio >24s at natural speech pauses, converts each chunk,
        and joins them with micro-fading to ensure high-fidelity voice conversion for long audios.
        """
        if not self.is_available():
            raise RuntimeError("CosyVoice model is not loaded or unavailable")

        if not os.path.exists(source_audio_path):
            raise FileNotFoundError(f"Source audio file not found: {source_audio_path}")
        if not os.path.exists(prompt_audio_path):
            raise FileNotFoundError(f"Prompt reference voice not found: {prompt_audio_path}")

        os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)

        # 1. Ensure prompt audio is not longer than 20s (CosyVoice reference voice sweet spot is 3-15s)
        effective_prompt_path = prompt_audio_path
        temp_prompt_file = None
        try:
            import soundfile as sf
            prompt_info = sf.info(prompt_audio_path)
            if prompt_info.duration > 20.0:
                logger.info(f"✂️ Trimming reference prompt voice from {prompt_info.duration:.2f}s to 15s...")
                p_data, p_sr = sf.read(prompt_audio_path, frames=int(15.0 * prompt_info.samplerate), dtype='float32')
                temp_prompt_file = output_path + ".tmp_prompt.wav"
                sf.write(temp_prompt_file, p_data, p_sr)
                effective_prompt_path = temp_prompt_file
        except Exception as e:
            logger.warning(f"Could not inspect prompt duration ({e}), using as-is.")
            effective_prompt_path = prompt_audio_path

        try:
            import soundfile as sf
            source_data, sr = sf.read(source_audio_path, dtype='float32')
            total_duration = len(source_data) / sr

            # Single chunk fast-path (audio <= 24s)
            if total_duration <= 24.0:
                logger.info(
                    f"🎙️ Running CosyVoice VC ({total_duration:.2f}s): source={source_audio_path}, prompt={effective_prompt_path}..."
                )
                converted_tensor = self._convert_single_chunk(source_audio_path, effective_prompt_path)
                self._save_audio(converted_tensor, output_path)
                logger.info(f"✅ CosyVoice conversion completed: {output_path}")
                return output_path

            # Multi-chunk speech-aware path for long audios (>24s)
            split_samples = _find_speech_split_points(source_data, sr, max_chunk_sec=24.0)
            num_chunks = len(split_samples) - 1
            logger.info(
                f"🎙️ Long audio detected ({total_duration:.2f}s). Splitting into {num_chunks} natural speech chunks (<=24s each)..."
            )

            chunk_tensors = []
            work_dir = os.path.dirname(os.path.abspath(output_path))
            with tempfile.TemporaryDirectory(dir=work_dir) as tmp_chunk_dir:
                for idx in range(num_chunks):
                    start_s = split_samples[idx]
                    end_s = split_samples[idx + 1]
                    chunk_dur = (end_s - start_s) / sr
                    chunk_file = os.path.join(tmp_chunk_dir, f"chunk_{idx:03}.wav")

                    sf.write(chunk_file, source_data[start_s:end_s], sr)
                    logger.info(f"🎙️ Converting speech chunk {idx + 1}/{num_chunks} ({chunk_dur:.2f}s)...")

                    chunk_out = self._convert_single_chunk(chunk_file, effective_prompt_path)
                    chunk_out = _smooth_fade(chunk_out, sr=self.model.sample_rate, fade_ms=5)
                    chunk_tensors.append(chunk_out)

            final_speech = torch.cat(chunk_tensors, dim=-1)
            self._save_audio(final_speech, output_path)
            logger.info(f"✅ CosyVoice multi-chunk conversion completed ({total_duration:.2f}s converted): {output_path}")
            return output_path

        finally:
            if temp_prompt_file and os.path.exists(temp_prompt_file):
                try:
                    os.remove(temp_prompt_file)
                except OSError:
                    pass

