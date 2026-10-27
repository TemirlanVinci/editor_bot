import os
import logging
import torch
import torchaudio

logger = logging.getLogger("voice_service.demucs")

class DemucsEngine:
    """
    Vocal separator engine using Meta Demucs v4 (htdemucs).
    Extracts clean vocals from speech containing background music or noise.
    """
    def __init__(self, device: str = "cuda", model_name: str = "htdemucs"):
        if device in ("auto", "cuda") and torch.cuda.is_available():
            self.device = "cuda"
        elif device == "cuda" and not torch.cuda.is_available():
            logger.warning("⚠️ CUDA requested for Demucs but not available. Falling back to CPU.")
            self.device = "cpu"
        else:
            self.device = device or "cpu"
        self.model_name = model_name
        self._separator = None

    def _get_separator(self):
        if self._separator is None:
            logger.info(f"⏳ Initializing Demucs v4 ({self.model_name}) on device: {self.device}...")
            from demucs.api import Separator
            # Note: Do not pass custom segment (e.g. segment=10) as htdemucs requires its native 7.8s segment
            self._separator = Separator(
                model=self.model_name,
                device=self.device,
                shifts=1,
                overlap=0.25,
            )
            logger.info("✅ Demucs v4 Separator initialized successfully.")
        return self._separator

    def separate_vocals(self, input_path: str, output_path: str) -> str:
        """
        Separates vocal track from input_path and saves it to output_path.
        Returns the output_path on success.
        """
        if not os.path.exists(input_path):
            raise FileNotFoundError(f"Input audio file not found: {input_path}")

        os.makedirs(os.path.dirname(os.path.abspath(output_path)), exist_ok=True)

        try:
            separator = self._get_separator()
            logger.info(f"🎤 Separating vocals with Demucs v4 for {input_path}...")
            _, stems = separator.separate_audio_file(input_path)

            if "vocals" not in stems:
                raise KeyError(f"Demucs stems do not contain 'vocals'. Available: {list(stems.keys())}")

            vocals_tensor = stems["vocals"]
            # Convert to CPU tensor if on GPU
            if vocals_tensor.is_cuda:
                vocals_tensor = vocals_tensor.cpu()

            # Save with soundfile for stability (avoiding torchcodec dependency in torchaudio)
            try:
                import soundfile as sf
                audio_np = vocals_tensor.numpy()
                if audio_np.ndim == 2:
                    audio_np = audio_np.T
                sf.write(output_path, audio_np, separator.samplerate)
            except Exception:
                torchaudio.save(output_path, vocals_tensor, separator.samplerate)

            logger.info(f"✅ Clean vocals successfully extracted to {output_path}")
            return output_path
        except Exception as e:
            logger.error(f"❌ Demucs API separation failed: {e}. Trying CLI fallback...")
            # Fallback via CLI execution
            import subprocess
            cmd = [
                "demucs",
                "--two-stems", "vocals",
                "-n", self.model_name,
                "-d", self.device,
                "-o", os.path.dirname(output_path),
                input_path
            ]
            res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            if res.returncode != 0:
                raise RuntimeError(f"Demucs CLI failed: {res.stderr}")
            # Locate output file
            filename = os.path.splitext(os.path.basename(input_path))[0]
            expected_cli_out = os.path.join(os.path.dirname(output_path), self.model_name, filename, "vocals.wav")
            if os.path.exists(expected_cli_out):
                if expected_cli_out != output_path:
                    os.replace(expected_cli_out, output_path)
                return output_path
            raise RuntimeError(f"Demucs output not found at {expected_cli_out}")
