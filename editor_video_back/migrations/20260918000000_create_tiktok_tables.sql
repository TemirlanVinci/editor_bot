CREATE TABLE IF NOT EXISTS accounts (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    cookies_path VARCHAR(512) NOT NULL,
    proxy_url VARCHAR(512) NOT NULL DEFAULT '', -- Format: http://user:pass@ip:port
    publish_time TIME DEFAULT '13:00:00', -- Default daily release time
    interval_days INT DEFAULT 1, -- Days between parts
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS queue (
    id SERIAL PRIMARY KEY,
    account_id INT REFERENCES accounts(id) ON DELETE CASCADE,
    file_path VARCHAR(512) NOT NULL,
    caption TEXT NOT NULL,
    scheduled_at TIMESTAMP NOT NULL,
    status VARCHAR(50) DEFAULT 'pending', -- pending, uploading, published, failed
    error_log TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_queue_pending ON queue(account_id, status, scheduled_at);

