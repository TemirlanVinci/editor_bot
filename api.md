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
1. **Whisper Transcription**: Единоразовое распознавание речи всего видео с извлечением пословных таймкодов, границ предложений и пауз.
2. **Context Analysis (Qwen 3.5 9B / 3B через Ollama)**: Смысловой анализ сюжета и разделение на логические части:
   - **Хук (тизер)**: Первые 4–12 секунд с главной интригой ("Am I the asshole for...").
   - **Истории**: Индивидуальные истории Reddit переменной длительности, разделенные сменой тем и естественными паузами речи.
3. **FFmpeg Video Cutting**: Динамическая нарезка, наложение случайного фонового видео, караоке-субтитров (ASS) и фоновой музыки с ускорением 1.08x.

Возвращает ZIP-архив с готовыми клипами (`fragment_1_final.mp4`, `fragment_2_final.mp4`...) и файлом метаданных `segments.json`.

### Ответы:
- **HTTP 200 OK** (`Content-Type: application/zip`): Бинарный поток ZIP-архива нарезанных клипов и `segments.json`.
- **HTTP 400 Bad Request**: Отсутствует поле `video` или переданы невалидные параметры.
- **HTTP 500 Internal Server Error**: Ошибка инференса Whisper / LLM / FFmpeg.

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
