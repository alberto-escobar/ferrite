# Ferrite

Ferrite is a self-hosted music streaming server written in Rust. Point it at a folder of audio files and stream your own library from any browser.

---

## Architecture

```
Your Music Files (MP3 / FLAC / OGG)
        │
        ▼
┌─────────────────────────────┐
│      Rust Backend (Axum)    │
│                             │
│  • File scanner & indexer   │
│  • SQLite library (metadata)│
│  • HTTP audio streaming     │
│  • REST API                 │
└────────────┬────────────────┘
             │ HTTP
             ▼
┌─────────────────────────────┐
│    Web UI (served by Axum)  │
│  • Library browser          │
│  • Player (HTML5 Audio)     │
│  • Playlist management      │
└─────────────────────────────┘
```
*used Claude to help generate diagram*

On startup, Ferrite scans your music directory, reads track metadata (title, artist, album, duration), and indexes everything into a local SQLite database. From there, all library queries hit the database rather than the filesystem. Audio is streamed on demand using HTTP range requests so seeking works instantly and only the parts of a file being played are transferred.

---

Decisions Made in This Project

HTTP Range Requests for streaming

Audio is served using HTTP range requests rather than sending the full file upfront. When the browser's <audio> element requests a track, it asks for small chunks at a time as playback progresses. The server responds with a 206 Partial Content status and only the requested byte range. This means playback starts immediately without waiting for a full download, seeking jumps directly to the relevant byte offset without re-transferring earlier parts of the file, and server memory stays low since files are never loaded whole into RAM. This is the same mechanism commercial streaming services use.

SQLite over Postgres

Ferrite uses SQLite as its database rather than a dedicated server like Postgres. Since Ferrite is self-hosted for personal use, SQLite is a single file on disk, requires no separate process to install or manage, and starts up as part of the application. For a low-concurrency read-heavy workload like a personal music library it performs well, and if the database file is ever lost it rebuilds completely from a rescan in seconds.

---

## API Reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/tracks` | List all tracks |
| `GET` | `/api/tracks/:id` | Get a single track's metadata |
| `GET` | `/api/tracks/:id/stream` | Stream audio (range-request aware) |
| `GET` | `/api/albums` | List all albums |
| `GET` | `/api/artists` | List all artists |
| `POST` | `/api/scan` | Trigger a library rescan |