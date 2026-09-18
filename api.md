## POST /api/v1/video/cut
Content-Type: multipart/form-data
X-Bot-Secret: <secret>

Multipart-поле:

video = original.mp4

Назначение

Принимает одно видео, разделяет его на фрагменты длительностью 60 секунд с перекрытием соседних фрагментов на 2 секунды и возвращает все созданные фрагменты в ZIP-архиве.

HTTP/1.1 200 OK
Content-Type: application/zip

В теле ответа:

fragments.zip
├── fragment_001.mp4
├── fragment_002.mp4
├── fragment_003.mp4
├── ...

---

## POST /api/v1/video/download
Content-Type: application/json
X-Bot-Secret: <secret>

Тело запроса (JSON):

```json
{
  "url": "https://www.youtube.com/watch?v=..."
}
```

Назначение

Принимает URL видео или YouTube Shorts, скачивает его с помощью `yt-dlp` (в наилучшем качестве до 1080p в формате MP4) и возвращает бинарный поток видеофайла.

Ответы:

### HTTP/1.1 200 OK
Content-Type: video/mp4

В теле ответа: Бинарный поток видео MP4.

### HTTP/1.1 400 Bad Request
Content-Type: text/plain

Передана невалидная или неподдерживаемая ссылка на YouTube.

### HTTP/1.1 500 Internal Server Error
Content-Type: text/plain

Ошибка при выполнении `yt-dlp` или сбой обработки на сервере.
