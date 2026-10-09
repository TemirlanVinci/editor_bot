-- Ensure column exists before seeding
ALTER TABLE accounts
ADD COLUMN IF NOT EXISTS publish_times VARCHAR(512) DEFAULT '13:00';
-- Seed sample TikTok accounts for testing and initial setup
INSERT INTO accounts (
        name,
        cookies_path,
        proxy_url,
        publish_times,
        interval_days,
        is_active
    )
VALUES (
        'Anlog 1 (KG)',
        '/app/media/acc1.json',
        'http://user:pass@185.123.45.67:8080',
        '9:10, 14:00, 19:00',
        1,
        true
    ),
    (
        'Anlog 2 (KG)',
        '/app/media/acc2.json',
        'http://user:pass@185.123.45.68:8080',
        '12:30, 18:06',
        1,
        true
    ),
    (
        'Anlog 3 (KG)',
        '/app/media/acc3.json',
        'http://user:pass@185.123.45.69:8080',
        '15:00',
        1,
        true
    ) ON CONFLICT DO NOTHING;