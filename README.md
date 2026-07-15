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
└───────┬─────────────────────┘
        │ HTTP
        ▼
┌─────────────────────────────┐
│    Web UI (served by Axum)  │
│  • Library browser          │
│  • Player (HTML5 Audio)     │
└─────────────────────────────┘
```

On startup, Ferrite scans your music directory, reads track metadata (title, artist, album, duration), and indexes everything into a local SQLite database. From there, all library queries hit the database rather than the filesystem. Audio is streamed on demand using HTTP range requests so seeking works instantly and only the parts of a file being played are transferred.

---

Decisions Made in This Project

HTTP Range Requests for streaming

Audio is served using HTTP range requests rather than sending the full file upfront. When the browser's <audio> element requests a track, it asks for small chunks at a time as playback progresses. The server responds with a 206 Partial Content status and only the requested byte range. This means playback starts immediately without waiting for a full download, seeking jumps directly to the relevant byte offset without re-transferring earlier parts of the file, and server memory stays low since files are never loaded whole into RAM. This is the same mechanism commercial streaming services use.

SQLite over Postgres

Ferrite uses SQLite as its database rather than a dedicated server like Postgres. Since Ferrite is self-hosted for personal use, SQLite is a single file on disk, requires no separate process to install or manage, and starts up as part of the application. For a low-concurrency read-heavy workload like a personal music library it performs well, and if the database file is ever lost it rebuilds completely from a rescan in seconds.

Docker for deployment

Ferrite ships with a production Dockerfile (`Dockerfile.prod`) that builds a small ARM64 image for running on a Raspberry Pi, plus a local Dockerfile/compose setup for running in a container on a dev machine. Packaging it as a container means the server, its runtime dependencies, and the SQLite data volume are all deployed as one unit, so shipping an update to the Pi is just pulling a new image rather than reconciling toolchains by hand. Since Rust compiles to a self-contained binary, Ferrite can just as easily be run directly on the host without Docker — the container is a deployment convenience, not a requirement.

---

## API Reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/tracks` | List all tracks |
| `GET` | `/api/tracks/:id` | Get a single track's metadata |
| `GET` | `/api/tracks/:id/stream` | Stream audio (range-request aware) |
| `GET` | `/api/albums` | List all albums |
| `GET` | `/api/artists` | List all artists |
| `GET` | `/api/downloads` | List all queued/completed downloads |
| `POST` | `/api/fetch` | Fetch a song from a URL (via yt-dlp) and add it to the library |

---

## CI/CD

Ferrite deploys to a Raspberry Pi running the container as a systemd/Docker service, kept up to date automatically:

- **Build** — a GitHub Actions workflow (`.github/workflows/deploy.yml`) runs on every push to `main`. It cross-compiles the release binary for `aarch64-unknown-linux-gnu`, builds a `linux/arm64` Docker image from `Dockerfile.prod`, and pushes it to GitHub Container Registry (`ghcr.io`).
- **Deploy** — the Pi runs [WUD (What's Up Docker)](https://github.com/getwud/wud), which watches the `ferrite` image on GHCR. When a new tag is pushed, WUD pulls it and redeploys the container automatically, so there's no manual SSH-and-restart step.
- **Local dev** — a separate Dockerfile/compose setup is used for running Ferrite in a container locally. Since Rust builds a self-contained binary with all its dependencies, Ferrite can also just be run directly on the host with `cargo run`/`cargo build --release` without Docker at all.


## TODO
- [x] Axum web server serving a web UI
- [x] SQLite database with automatic migrations on startup
- [x] Music directory scanner with metadata extraction
- [x] REST API for tracks, albums and artists
- [x] HTTP audio streaming with range request support
- [x] Spotify-style web UI with playback controls, seek bar and volume
- [x] Docker containerization with local dev and production Dockerfiles
- [x] GitHub Actions CI/CD pipeline with ARM64 cross-compilation
- [x] Auto-deployment to Raspberry Pi via WUD (What's Up Docker)
- [x] YouTube import via yt-dlp with background job queue
- [x] AI metadata inference for imported tracks
- [ ] Clean up UI and add logo 
- [ ] Enable public access via Nginx reverse proxy + Let's Encrypt HTTPS + DuckDNS
- [ ] Optimize music directory scanner by using diff system
