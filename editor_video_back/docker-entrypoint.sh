#!/bin/sh
set -e

# Проверяем наличие модели Whisper по пути WHISPER_MODEL_PATH.
# Если файл отсутствует (например, при первом запуске с чистым томом whisper_models),
# скачиваем его с HuggingFace перед стартом бэкенда.
if [ -n "$WHISPER_MODEL_PATH" ] && [ ! -f "$WHISPER_MODEL_PATH" ]; then
    echo "⬇️ Whisper model not found at $WHISPER_MODEL_PATH. Downloading..."
    mkdir -p "$(dirname "$WHISPER_MODEL_PATH")"
    MODEL_FILE=$(basename "$WHISPER_MODEL_PATH")
    curl -fL "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${MODEL_FILE}" -o "$WHISPER_MODEL_PATH"
    echo "✅ Whisper model successfully downloaded to $WHISPER_MODEL_PATH"
fi

exec "$@"
