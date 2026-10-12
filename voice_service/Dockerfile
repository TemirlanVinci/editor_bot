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

# Install CosyVoice repository if needed
RUN git clone --recursive https://github.com/FunAudioLLM/CosyVoice.git /opt/CosyVoice || true
ENV PYTHONPATH="/app:/opt/CosyVoice:/opt/CosyVoice/third_party/Matcha-TTS"

COPY . .

EXPOSE 8082

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8082"]
