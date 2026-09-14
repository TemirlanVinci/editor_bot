FROM rust:latest as builder

WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY . .

ENV SQLX_OFFLINE=true
RUN cargo build --release

FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# ТУТ ЗАМЕНИЛ imperia_pizza_back НА editor_video_back
COPY --from=builder /app/target/release/editor_video_back /app/editor_video_back

EXPOSE 8080

# ТУТ ТОЖЕ
CMD ["./editor_video_back"]