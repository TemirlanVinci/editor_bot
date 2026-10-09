-- Ensure column exists before seeding
ALTER TABLE accounts
ADD COLUMN IF NOT EXISTS publish_times VARCHAR(512) DEFAULT '13:00';

-- Seed / synchronize TikTok accounts from this file
CREATE TEMP TABLE temp_seed_accounts (
    id INT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    cookies_path VARCHAR(512) NOT NULL,
    proxy_url VARCHAR(512) NOT NULL DEFAULT '',
    publish_times VARCHAR(512) DEFAULT '13:00',
    interval_days INT DEFAULT 1,
    is_active BOOLEAN DEFAULT TRUE
) ON COMMIT DROP;

INSERT INTO temp_seed_accounts (id, name, cookies_path, proxy_url, publish_times, interval_days, is_active)
VALUES (
    1,
    'Anlog 1 (KG)',
    '/app/media/acc1.json',
    'http://user:pass@185.123.45.67:8080',
    '9:10, 14:00, 20:00',
    1,
    true
),
(
    2,
    'Anlog 2 (KG)',
    '/app/media/acc2.json',
    'http://user:pass@185.123.45.68:8080',
    '12:30, 18:06',
    1,
    true
),
(
    3,
    'Anlog 3 (KG)',
    '/app/media/acc3.json',
    'http://user:pass@185.123.45.69:8080',
    '15:00',
    1,
    true
);

-- 1. Insert new or update existing accounts by ID
INSERT INTO accounts (id, name, cookies_path, proxy_url, publish_times, interval_days, is_active)
SELECT id, name, cookies_path, proxy_url, publish_times, interval_days, is_active FROM temp_seed_accounts
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    cookies_path = EXCLUDED.cookies_path,
    proxy_url = EXCLUDED.proxy_url,
    publish_times = EXCLUDED.publish_times,
    interval_days = EXCLUDED.interval_days,
    is_active = EXCLUDED.is_active;

-- 2. Remove any obsolete accounts not present in the seed list
DELETE FROM accounts WHERE id NOT IN (SELECT id FROM temp_seed_accounts);

-- 3. Synchronize auto-increment sequence
SELECT setval(pg_get_serial_sequence('accounts', 'id'), COALESCE((SELECT MAX(id) FROM accounts), 1));