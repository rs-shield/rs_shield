-- Create files table for Drive-like metadata
CREATE TABLE IF NOT EXISTS files (
    id SERIAL PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    blake3_hash TEXT NOT NULL,
    size BIGINT NOT NULL,
    version INTEGER DEFAULT 1,
    modified_at TIMESTAMPTZ DEFAULT NOW(),
    owner_id INTEGER,
    is_deleted BOOLEAN DEFAULT FALSE
);

CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_hash ON files(blake3_hash);
