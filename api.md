# API Documentation (`editor_video_back`)

Все боевые эндпоинты в префиксе `/api/v1` защищены обязательным HTTP-заголовком:
```http
X-Bot-Secret: <BOT_SECRET>
```
При отсутствии или неверном значении заголовка возвращается `HTTP 401 Unauthorized`.

Служебный эндпоинт проверки работоспособности `/health` открыт без авторизации.

---

## GET /health

### Назначение
Используется Docker Compose, балансировщиком нагрузки или внешним мониторингом для проверки готовности сервиса. Заголовок `X-Bot-Secret` **не требуется**.

### Ответ:
- **HTTP 200 OK** (`Content-Type: text/plain`): `OK`

---

## POST /api/v1/video/download
Content-Type: application/json  
X-Bot-Secret: `<BOT_SECRET>`

### Тело запроса (JSON):
```json
{
  "url": "https://www.youtube.com/watch?v=..."
}
```

### Назначение
Принимает URL видео или YouTube Shorts, скачивает его с помощью `yt-dlp` (до 1080p в формате MP4) и возвращает бинарный поток видеофайла.

### Ответы:
- **HTTP 200 OK** (`Content-Type: video/mp4`): Бинарный поток MP4.
- **HTTP 400 Bad Request**: Невалидная ссылка или ошибка валидации запроса.
- **HTTP 500 Internal Server Error**: Ошибка при скачивании `yt-dlp` или ошибке файловой системы.

---

## POST /api/v1/video/cut
Content-Type: multipart/form-data  
X-Bot-Secret: `<BOT_SECRET>`

### Параметры запроса:
- **Query / Multipart-поле:** `include_intro` (boolean, опционально, по умолчанию `true`):
  - `true`: полный пайплайн с вырезкой вводного хука и склейкой `[intro] + [story]` для каждого клипа.
  - `false`: вырезка только историй `[start_timestamp, end_timestamp]` без склейки вводной части.
- **Multipart-поле:** `video` (бинарный видеофайл, e.g. `input.mp4`)

### Назначение
Принимает видеофайл и выполняет пайплайн смысловой обработки с помощью ИИ:
1. **Whisper Transcription**: Единоразовое распознавание речи всего исходного видео (включая видео с фоновой музыкой) с извлечением пословных таймкодов, границ предложений и пауз.
2. **Context Analysis (Qwen 2.5 3B / Ollama)**: Смысловой анализ сюжета и разделение на логические части:
   - **Хук (тизер)**: Первые секунды с заглавным вопросом ("Am I the asshole for...").
   - **Истории**: Индивидуальные истории Reddit переменной длительности с авто-кастингом голосов (`male_adult`, `male_young`, `female_adult`, `female_young`) и разбиением на реплики персонажей (`voice_segments`).
3. **Audio Extraction & Cleaning (Demucs v4)**: FFmpeg вырезает точные куски аудио истории, а модель Demucs v4 на CPU очищает речь от оригинальной фоновой музыки YouTube, получая чистый вокал `vocals.wav`.
4. **Voice Conversion (CosyVoice 3 VC)**: Модель CosyVoice 3 в режиме `inference_vc` берет чистый `vocals.wav` и меняет тембр голоса диктора на референсный голос из `media/voices/<voice>/`, сохраняя 100% исходной длительности, пауз и слогов.
5. **FFmpeg Video Assembly**: Динамический монтаж видео: наложение случайного фонового видео (B-roll), нового фонового бита, караоке-субтитров (ASS) и ускорение 1.08x (анти-фрод мутация).

Возвращает ZIP-архив с готовыми клипами (`fragment_1_final.mp4`, `fragment_2_final.mp4`...) и файлом метаданных `segments.json`.

### Ответы:
- **HTTP 200 OK** (`Content-Type: application/zip`): Бинарный поток ZIP-архива нарезанных клипов и `segments.json`.
- **HTTP 400 Bad Request**: Отсутствует поле `video` или переданы невалидные параметры.
- **HTTP 500 Internal Server Error**: Ошибка инференса Whisper / LLM / FFmpeg / Voice.

---

## GET /api/v1/accounts
X-Bot-Secret: `<BOT_SECRET>`

### Назначение
Возвращает список всех активных аккаунтов TikTok из таблицы `accounts`. Поле `publish_time` содержит либо одно время, либо список слотов публикации через запятую.

### Ответ (HTTP 200 OK):
```json
[
  {
    "id": 1,
    "name": "Аккаунт #1 (RU)",
    "cookies_path": "/app/media/cookies_acc1.json",
    "proxy_url": "http://user:pass@185.123.45.67:8080",
    "publish_time": "13:00, 18:00",
    "interval_days": 1,
    "is_active": true
  }
]
```

---

## POST /api/v1/accounts
Content-Type: application/json  
X-Bot-Secret: `<BOT_SECRET>`

### Тело запроса (JSON):
```json
{
  "name": "Аккаунт_1",
  "cookies_path": "/app/media/cookies_acc1.json",
  "proxy_url": "http://user:pass@ip:port",
  "publish_time": "13:00, 18:00",
  "interval_days": 1,
  "is_active": true
}
```

### Назначение
Добавляет новый аккаунт TikTok в базу данных. Поле `publish_time` поддерживает несколько слотов через запятую (например, `10:00, 15:00, 20:00`).

### Ответ (HTTP 201 Created):
Объект созданного аккаунта с присвоенным `id`.

---

## GET /api/v1/accounts/{id}
X-Bot-Secret: `<BOT_SECRET>`

### Назначение
Получить данные аккаунта по его ID.

### Ответы:
- **HTTP 200 OK**:
```json
{
  "id": 1,
  "name": "Аккаунт #1 (RU)",
  "cookies_path": "/app/media/cookies_acc1.json",
  "proxy_url": "http://user:pass@ip:port",
  "publish_time": "13:00, 18:00",
  "interval_days": 1,
  "is_active": true
}
```
- **HTTP 404 Not Found**: Аккаунт с указанным `id` не существует.

---

## DELETE /api/v1/accounts/{id}
X-Bot-Secret: `<BOT_SECRET>`

### Назначение
Удалить аккаунт по ID из базы данных (каскадно удаляет связанные записи в очереди публикаций).

### Ответы:
- **HTTP 204 No Content**: Успешно удален.
- **HTTP 404 Not Found**: Аккаунт не найден.

---

## DELETE /api/v1/accounts/{id}/videos
X-Bot-Secret: `<BOT_SECRET>`

### Назначение
Удалить все видео из архива аккаунта: физические файлы в папке `/app/media/acc_<account_id>/` и все задачи очереди `queue` для данного аккаунта.

### Ответы:
- **HTTP 200 OK**:
```json
{
  "deleted_count": 10,
  "account_id": 1
}
```
- **HTTP 400 Bad Request / 404 Not Found**: Аккаунт не найден.

---

## POST /api/v1/queue/schedule
Content-Type: application/json  
X-Bot-Secret: `<BOT_SECRET>`

### Тело запроса (JSON):
```json
{
  "account_id": 1,
  "job_id": "8096030502_6fd4fa5b"
}
```

### Назначение
Распределяет все нарезанные клипы из временной директории `/app/media/tmp/job_<job_id>/` в постоянную директорию аккаунта `/app/media/acc_<account_id>/`.
Считывает заголовки из `segments.json`, добавляет случайные хэштеги из таблицы `hashtags`, рассчитывает хронологию публикаций по слотам аккаунта с естественным человеческим джиттером минут и секунд, и создает записи в таблице `queue` со статусом `pending`.

### Ответ (HTTP 200 OK):
```json
{
  "scheduled_count": 5,
  "first_scheduled_at": "2026-10-08 13:04:17"
}
```

---

## POST /api/v1/queue/claim_due
X-Bot-Secret: `<BOT_SECRET>`

### Назначение
Вызывается фоновым воркером TikTok (`worker.py`). Находит самую старую невыполненную задачу (`pending`), у которой время `scheduled_at <= NOW()`. Атомарно переводит статус в `uploading` и возвращает путь к файлу, прокси и куки. Также автоматически сбрасывает зависшие задачи в статусе `uploading` старше 20 минут обратно в очередь.

### Ответы:
- **HTTP 200 OK**:
```json
{
  "id": 105,
  "account_id": 1,
  "file_path": "/app/media/acc_1/job_12345_part_01.mp4",
  "caption": "AITA for wedding refusal... | #fyp #viral #reddit",
  "scheduled_at": "2026-10-08 13:04:17",
  "proxy_url": "http://user:pass@ip:port",
  "cookies_path": "/app/media/cookies_acc1.json",
  "segment_id": 1,
  "segment_type": "story",
  "start_time": 0.0,
  "end_time": 45.8,
  "title": "AITA for wedding refusal"
}
```
- **HTTP 204 No Content**: Задач, готовых к публикации, в данный момент нет.

---

## POST /api/v1/queue/update_status
Content-Type: application/json  
X-Bot-Secret: `<BOT_SECRET>`

### Тело запроса (JSON):
```json
{
  "task_id": 105,
  "status": "published",
  "error_log": null
}
```

### Назначение
Обновляет статус задачи после попытки публикации (`published` или `failed`).
При статусе `published` сервер автоматически физически удаляет видеофайл с диска (`/app/media/acc_<id>/...`), освобождая место на сервере.

### Ответ (HTTP 200 OK):
Статус обновлен.

---

## GET /api/v1/hashtags/random?count=5
X-Bot-Secret: `<BOT_SECRET>`

### Назначение
Возвращает список случайных хэштегов из таблицы `hashtags`.
Параметр `count` (опционально, по умолчанию `5`) задает количество возвращаемых хэштегов.

### Ответ (HTTP 200 OK):
```json
[
  "#reddit",
  "#реддитистории",
  "#askreddit",
  "#историиизжизни",
  "#reddittok"
]
```

---

## Микросервис обработки голоса (`voice_service`: Demucs v4 + CosyVoice 3)

Внутренний микросервис (порт `8082`), работающий в связке с Rust-бэкендом для разделения дорожек и изменения тембра.

### GET /health
Проверка статуса моделей и наличия референсных голосов в `media/voices/`:
- **HTTP 200 OK**:
```json
{
  "status": "ok",
  "cosyvoice_loaded": true,
  "demucs_device": "cpu",
  "voices_dir": "/app/media/voices",
  "available_voices": {
    "male_adult": true,
    "male_young": true,
    "female_adult": true,
    "female_young": true
  }
}
```

### POST /api/v1/voice/separate_vocals
Очистка вокала от фоновой музыки через Meta Demucs v4 (`htdemucs`):
```json
{
  "source_audio_path": "/app/media/temp/input.wav",
  "output_audio_path": "/app/media/temp/vocals.wav"
}
```

### POST /api/v1/voice/convert
Комплексный пайплайн: опциональная очистка вокала (Demucs) + конверсия тембра (CosyVoice 3 `inference_vc`):
```json
{
  "source_audio_path": "/app/media/temp/raw_chunk.wav",
  "output_audio_path": "/app/media/temp/converted_chunk.wav",
  "voice": "male_adult",
  "enable_demucs": true
}
```
**Ответ:**
```json
{
  "status": "success",
  "output_path": "/app/media/temp/converted_chunk.wav",
  "voice_applied": "male_adult",
  "demucs_applied": true,
  "message": "Voice converted to 'male_adult' successfully."
}
```
*Примечание:* Если референсный файл в `media/voices/<voice>/` отсутствует, сервис возвращает `status: "fallback"` и отдает чистый вокал без изменения тембра, предотвращая сбой видеомонтажа.
