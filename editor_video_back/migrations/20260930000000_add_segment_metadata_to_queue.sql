-- Add segment metadata columns to queue table for narrative segmentation
ALTER TABLE queue ADD COLUMN IF NOT EXISTS segment_id INT;
ALTER TABLE queue ADD COLUMN IF NOT EXISTS segment_type VARCHAR(50);
ALTER TABLE queue ADD COLUMN IF NOT EXISTS start_time DOUBLE PRECISION;
ALTER TABLE queue ADD COLUMN IF NOT EXISTS end_time DOUBLE PRECISION;
ALTER TABLE queue ADD COLUMN IF NOT EXISTS title VARCHAR(255);
