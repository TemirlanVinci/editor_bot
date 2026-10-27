#!/bin/sh
set -e

TARGET_DIR="${COSYVOICE_MODEL_DIR:-/app/models/CosyVoice-300M}"

# Ensure CosyVoice-300M model is present on the persistent volume
if [ ! -d "$TARGET_DIR" ] || [ -z "$(ls -A "$TARGET_DIR" 2>/dev/null)" ]; then
    echo "⬇️ CosyVoice-300M model not found at $TARGET_DIR. Downloading from ModelScope..."
    mkdir -p "$TARGET_DIR"
    python -c "from modelscope import snapshot_download; snapshot_download('iic/CosyVoice-300M', local_dir='$TARGET_DIR')"
    echo "✅ CosyVoice-300M model successfully downloaded to $TARGET_DIR"
fi

exec "$@"
