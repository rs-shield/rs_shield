-- Create files table for metadata
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    hash TEXT NOT NULL,
    size INTEGER NOT NULL,
    modified_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    version INTEGER DEFAULT 1,
    user_id INTEGER,
    metadata TEXT
);

CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_hash ON files(hash);
