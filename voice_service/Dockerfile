FROM python:3.10-slim

ENV PYTHONUNBUFFERED=1 \
    DEBIAN_FRONTEND=noninteractive

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    git \
    sox \
    libsox-dev \
    build-essential \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .

RUN pip install --no-cache-dir -r requirements.txt

# Install CosyVoice repository and Matcha-TTS
RUN git clone --recursive https://github.com/FunAudioLLM/CosyVoice.git /opt/CosyVoice || true && \
    (cd /opt/CosyVoice/third_party/Matcha-TTS && pip install --no-cache-dir -e . || true)

ENV PYTHONPATH="/app:/opt/CosyVoice:/opt/CosyVoice/third_party/Matcha-TTS"

COPY . .
RUN chmod +x docker-entrypoint.sh

EXPOSE 8082

ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8082"]
