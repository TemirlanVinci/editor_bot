FROM rust:1-slim-bookworm as builder

WORKDIR /app

# pkg-config/libssl-dev — как и раньше, для sqlx/tls.
# cmake/clang/libclang-dev/git — нужны whisper-rs, который на этапе сборки
# компилирует C/C++ исходники whisper.cpp (через bindgen + cc/cmake).
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    clang \
    libclang-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

COPY . .

ENV SQLX_OFFLINE=true
RUN cargo build --release


FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    ffmpeg \
    libgomp1 \
    fontconfig \
    fonts-noto-core \
    curl \
    python3 \
    unzip \
    && curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp \
    && curl -fsSL https://deno.land/install.sh | DENO_INSTALL=/usr/local sh \
    && rm -rf /var/lib/apt/lists/*

# Модель распознавания речи Whisper (мультиязычная, поддерживает русский).
# "large-v3-turbo-q5_0" — квантованная (Q5_0, ~574MB) модель:
# превосходное качество распознавания и точность таймкодов при высокой скорости инференса
# (см. https://huggingface.co/ggerganov/whisper.cpp).
ARG WHISPER_MODEL=large-v3-turbo-q5_0
RUN mkdir -p /app/models && \
    curl -fL "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${WHISPER_MODEL}.bin" \
    -o "/app/models/ggml-${WHISPER_MODEL}.bin"

ENV WHISPER_MODEL_PATH=/app/models/ggml-${WHISPER_MODEL}.bin
ENV WHISPER_LANGUAGE=ru

COPY --from=builder /app/target/release/editor_video_back /app/editor_video_back
COPY docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh

EXPOSE 8081

ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["./editor_video_back"]