-- Create background_videos table if it doesn't exist
CREATE TABLE IF NOT EXISTS background_videos (
    id SERIAL PRIMARY KEY,
    file_path VARCHAR(512) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

