FROM python:3.11-slim

# Запрещаем Python создавать .pyc файлы и включаем небуферизованный вывод логов
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

WORKDIR /app

# Копируем список зависимостей и устанавливаем их
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Копируем исходный код бота
COPY . .

CMD ["python", "bot/main.py"]