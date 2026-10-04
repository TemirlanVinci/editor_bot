-- Ensure background_videos file_path has a unique constraint for ON CONFLICT (file_path)
DO $$
BEGIN
    -- 1. Remove duplicate file_path entries if any exist (keeping lowest id)
    DELETE FROM background_videos a
    USING background_videos b
    WHERE a.id > b.id AND a.file_path = b.file_path;

    -- 2. Add unique constraint if not already present
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'background_videos'::regclass
          AND contype = 'u'
    ) THEN
        ALTER TABLE background_videos ADD CONSTRAINT background_videos_file_path_key UNIQUE (file_path);
    END IF;
END $$;
