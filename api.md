# API Documentation (`editor_video_back`)

Все API-эндпоинты защищены заголовком `X-Bot-Secret: <secret>`.

---

## POST /api/v1/video/download
Content-Type: application/json  
X-Bot-Secret: <secret>

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
- **HTTP 400 Bad Request**: Невалидная ссылка.
- **HTTP 500 Internal Server Error**: Ошибка при скачивании `yt-dlp`.

---

## POST /api/v1/video/cut
Content-Type: multipart/form-data  
X-Bot-Secret: <secret>

### Multipart-поле:
`video` = original.mp4

### Назначение
Принимает видеофайл, разделяет его на клипы (60 секунд с наложением фонового видео, текста и караоке-субтитров) и возвращает ZIP-архив с готовыми частями (`part_01.mp4`, `part_02.mp4`...).

### Ответы:
- **HTTP 200 OK** (`Content-Type: application/zip`): ZIP-архив нарезанных клипов.

---

## GET /api/v1/accounts
X-Bot-Secret: <secret>

### Назначение
Возвращает список всех активных аккаунтов TikTok из таблицы `accounts`.

### Ответ (HTTP 200 OK):
```json
[
  {
    "id": 1,
    "name": "Аккаунт_1",
    "cookies_path": "/app/media/cookies_acc1.json",
    "proxy_url": "http://user:pass@ip:port",
    "publish_time": "13:00:00",
    "interval_days": 1,
    "is_active": true
  }
]
```

---

## POST /api/v1/accounts
Content-Type: application/json  
X-Bot-Secret: <secret>

### Тело запроса (JSON):
```json
{
  "name": "Аккаунт_1",
  "cookies_path": "/app/media/cookies_acc1.json",
  "proxy_url": "http://user:pass@ip:port",
  "publish_time": "13:00:00",
  "interval_days": 1,
  "is_active": true
}
```

### Назначение
Добавляет новый аккаунт TikTok в базу данных.

### Ответ (HTTP 201 Created):
Объект созданного аккаунта с присвоенным `id`.

---

## GET /api/v1/accounts/{id}
X-Bot-Secret: <secret>

### Назначение
Получить данные аккаунта по его ID.

---

## DELETE /api/v1/accounts/{id}
X-Bot-Secret: <secret>

### Назначение
Удалить аккаунт по ID из базы данных.

### Ответ:
- **HTTP 204 No Content**: Успешно удален.
- **HTTP 404 Not Found**: Аккаунт не найден.

---

## DELETE /api/v1/accounts/{id}/videos
X-Bot-Secret: <secret>

### Назначение
Удалить все видео из архива аккаунта (физические файлы и все задачи в очереди `queue`).

### Ответ (HTTP 200 OK):
```json
{
  "deleted_count": 10,
  "account_id": 1
}
```

---


## POST /api/v1/queue/schedule
Content-Type: application/json  
X-Bot-Secret: <secret>

### Тело запроса (JSON):
```json
{
  "account_id": 1,
  "job_id": "12345_abc123"
}
```

### Назначение
Распределяет все нарезанные клипы из временной директории `/app/media/tmp/job_<job_id>/` в постоянную директорию аккаунта `/app/media/acc_<account_id>/`.
Расчитывает точную хронологию публикаций для каждой части (по умолчанию ежедневно в `13:00:00`) и делает запись в таблицу `queue` со статусом `pending`.

### Ответ (HTTP 200 OK):
```json
{
  "scheduled_count": 10,
  "first_scheduled_at": "2026-09-20 13:00:00"
}
```

---

## POST /api/v1/queue/claim_due
X-Bot-Secret: <secret>

### Назначение
Вызывается фоновым воркером Playwright. Находит самую старую невыполненную задачу (`pending`), у которой время `scheduled_at <= NOW()`. Атомарно меняет статус на `uploading` и возвращает путь к файлу, прокси и куки.

### Ответ:
- **HTTP 200 OK**:
```json
{
  "id": 105,
  "account_id": 1,
  "file_path": "/app/media/acc_1/job_12345_part_01.mp4",
  "caption": "Part 1/10 | #fyp #viral",
  "scheduled_at": "2026-09-20 13:00:00",
  "proxy_url": "http://user:pass@ip:port",
  "cookies_path": "/app/media/cookies_acc1.json"
}
```
- **HTTP 204 No Content**: Задач, готовых к публикации, в данный момент нет.

---

## POST /api/v1/queue/update_status
Content-Type: application/json  
X-Bot-Secret: <secret>

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
При статусе `published` сервер автоматически физически удаляет файл роликов с диска (`/app/media/acc_1/...`), освобождая место на сервере.

---

## GET /api/v1/hashtags/random?count=5
X-Bot-Secret: <secret>

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
