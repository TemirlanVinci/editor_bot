FROM rust:1-slim-bookworm as builder

WORKDIR /app

# pkg-config/libssl-dev — для sqlx/tls.
# cmake/clang/libclang-dev/git — для whisper-rs (компиляция whisper.cpp через bindgen + cmake).
# nvidia-cuda-toolkit — для компиляции whisper.cpp с поддержкой CUDA (nvcc / GGML_CUDA).
RUN sed -i 's/Components: main/Components: main contrib non-free non-free-firmware/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
    make \
    gcc-11 \
    g++-11 \
    clang \
    libclang-dev \
    git \
    nvidia-cuda-toolkit \
    nvidia-cuda-dev \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /usr/local/cuda && \
    ln -s /usr/include /usr/local/cuda/include && \
    ln -s /usr/lib/x86_64-linux-gnu /usr/local/cuda/lib64 && \
    ln -s /usr/bin /usr/local/cuda/bin && \
    printf '#!/bin/sh\nexec /usr/bin/nvcc -ccbin /usr/bin/g++-11 "$@"\n' > /usr/local/bin/nvcc && \
    chmod +x /usr/local/bin/nvcc

ENV CUDA_PATH=/usr/local/cuda
ENV CUDAToolkit_ROOT=/usr/local/cuda
ENV CMAKE_CUDA_HOST_COMPILER=/usr/bin/g++-11
ENV CUDAHOSTCXX=/usr/bin/g++-11
ENV GGML_NATIVE=OFF
ENV CMAKE_CUDA_ARCHITECTURES=86

COPY . .

ENV SQLX_OFFLINE=true
RUN cargo build --release --features cuda


FROM debian:bookworm-slim

WORKDIR /app

RUN sed -i 's/Components: main/Components: main contrib non-free non-free-firmware/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    ffmpeg \
    libgomp1 \
    fontconfig \
    fonts-noto-core \
    curl \
    python3 \
    unzip \
    libcudart11.0 \
    libcublas11 \
    libcublaslt11 \
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
ENV WHISPER_USE_GPU=true
ENV NVIDIA_VISIBLE_DEVICES=all
ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility

COPY --from=builder /app/target/release/editor_video_back /app/editor_video_back
COPY docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh

EXPOSE 8081

ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["./editor_video_back"]