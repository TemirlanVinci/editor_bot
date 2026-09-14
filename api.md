##POST /api/v1/video/cut
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
