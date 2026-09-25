ALTER TABLE accounts ADD COLUMN IF NOT EXISTS publish_times VARCHAR(512) DEFAULT '13:00';
UPDATE accounts SET publish_times = publish_time::text WHERE publish_times IS NULL OR publish_times = '13:00:00' OR publish_times = '13:00';
