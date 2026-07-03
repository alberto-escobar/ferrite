CREATE TABLE IF NOT EXISTS tracks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT NOT NULL,
    artist       TEXT NOT NULL,
    album        TEXT,
    file_path    TEXT NOT NULL UNIQUE,
    duration     INTEGER,
    track_number INTEGER
);