FROM python:3.11-slim

# Prevent Python from writing .pyc files and enable unbuffered logging output
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

WORKDIR /app

# Copy requirements and install system dependencies for Playwright
COPY requirements.txt .
RUN apt-get update && apt-get install -y --no-install-recommends \
    wget \
    gnupg \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir -r requirements.txt
RUN playwright install --with-deps chromium

# Copy bot source code
COPY . .

CMD ["python", "bot/main.py"]