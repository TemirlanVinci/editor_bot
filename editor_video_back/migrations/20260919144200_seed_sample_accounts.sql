-- Seed sample TikTok accounts for testing and initial setup
INSERT INTO accounts (name, cookies_path, proxy_url, publish_time, interval_days, is_active)
VALUES 
    ('Аккаунт #1 (RU)', '/app/media/cookies_acc1.json', 'http://user:pass@185.123.45.67:8080', '13:00:00', 1, true),
    ('Аккаунт #2 (KZ)', '/app/media/cookies_acc2.json', 'http://user:pass@185.123.45.68:8080', '14:00:00', 1, true),
    ('Аккаунт #3 (US)', '/app/media/cookies_acc3.json', 'http://user:pass@185.123.45.69:8080', '15:00:00', 1, true)
ON CONFLICT DO NOTHING;
