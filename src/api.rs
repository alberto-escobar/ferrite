use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use sqlx::SqlitePool;
use crate::db;
use crate::metadata;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub async fn get_all_tracks(
    State(pool): State<SqlitePool>,
) -> Json<Vec<db::Track>> {
    let tracks = db::get_all_tracks(&pool).await;
    Json(tracks)
}

pub async fn get_track_by_id(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<Json<db::Track>, StatusCode> {
    match db::get_track_by_id(&pool, id).await {
        Some(track) => Ok(Json(track)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_all_albums(
    State(pool): State<SqlitePool>,
) -> Json<Vec<String>> {
    let albums = db::get_all_albums(&pool).await;
    Json(albums)
}

pub async fn get_all_artists(
    State(pool): State<SqlitePool>,
) -> Json<Vec<String>> {
    let artists = db::get_all_artists(&pool).await;
    Json(artists)
}

#[derive(serde::Deserialize)]
pub struct ImportRequest {
    pub url: String,
}

pub async fn get_downloads(
    State(pool): State<SqlitePool>,
) -> Json<Vec<db::Download>> {
    Json(db::get_all_downloads(&pool).await)
}

pub async fn fetch_song(
    State(pool): State<SqlitePool>,
    Json(body): Json<ImportRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let url = body.url.trim();

    // Reject empty input and anything that isn't a plain URL, since the value
    // is passed straight through as a yt-dlp argument (a leading "-" would be
    // interpreted as a flag).
    if url.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let url = url.to_string();
    let id = db::insert_download(&pool, &url).await;

    tokio::spawn(run_download(pool, id, url));

    Ok(Json(serde_json::json!({ "id": id, "status": "pending" })))
}

async fn run_download(pool: SqlitePool, id: i64, url: String) {
    db::update_download_status(&pool, id, "in_progress", None).await;

    // Everything — the raw download, yt-dlp's ffmpeg audio-extraction pass, and
    // the info-json dump — happens in this scratch dir. Nothing lands in Music/
    // until metadata generation and tagging succeed and we move the finished
    // file in ourselves.
    let temp_dir = std::env::temp_dir().join(format!("ferrite-yt-dlp-{id}"));

    let mut child = match Command::new("yt-dlp")
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg("--no-playlist")
        .arg("--write-info-json")
        .arg("-P")
        .arg(&temp_dir)
        .arg("-o")
        .arg("%(title)s.%(ext)s")
        .arg("--")
        .arg(&url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            db::update_download_status(&pool, id, "failed", Some(&e.to_string())).await;
            return;
        }
    };

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            println!("[yt-dlp:{id}] {line}");
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut collected = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            eprintln!("[yt-dlp:{id}] {line}");
            collected.push(line);
        }
        collected
    });

    let status = child.wait().await;
    let _ = stdout_task.await;
    let stderr_lines = stderr_task.await.unwrap_or_default();

    let result: Result<(), String> = match status {
        Ok(s) if s.success() => process_downloaded_file(&temp_dir, id).await,
        Ok(_) => Err(stderr_lines.join("\n")),
        Err(e) => Err(e.to_string()),
    };

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    match result {
        Ok(()) => {
            db::update_download_status(&pool, id, "done", None).await;
            crate::scanner::sync_library(&pool).await;
        }
        Err(e) => {
            eprintln!("[yt-dlp:{id}] failed: {e}");
            db::update_download_status(&pool, id, "failed", Some(&e)).await;
        }
    }
}

/// Locates yt-dlp's output in `temp_dir`, asks Gemini to generate clean tags
/// from the video's title/uploader/description, writes those tags into the
/// mp3, and moves the finished file into Music/.
async fn process_downloaded_file(temp_dir: &std::path::Path, id: i64) -> Result<(), String> {
    let mut mp3_path = None;
    let mut info_path = None;

    let mut entries = tokio::fs::read_dir(temp_dir)
        .await
        .map_err(|e| format!("Failed to read temp dir: {e}"))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read temp dir entry: {e}"))?
    {
        let path = entry.path();
        if path.to_string_lossy().ends_with(".info.json") {
            info_path = Some(path);
        } else if path.extension().and_then(|e| e.to_str()) == Some("mp3") {
            mp3_path = Some(path);
        }
    }

    let mp3_path = mp3_path.ok_or("yt-dlp did not produce an mp3 file")?;
    let info_path = info_path.ok_or("yt-dlp did not produce an info.json file")?;

    let info_json = tokio::fs::read_to_string(&info_path)
        .await
        .map_err(|e| format!("Failed to read info.json: {e}"))?;
    let video_info: metadata::VideoInfo = serde_json::from_str(&info_json)
        .map_err(|e| format!("Failed to parse yt-dlp info.json: {e}"))?;

    let song_metadata = metadata::generate_metadata(&video_info, id).await?;

    let tag_path = mp3_path.clone();
    tokio::task::spawn_blocking(move || write_tags(&tag_path, &song_metadata))
        .await
        .map_err(|e| format!("Tagging task panicked: {e}"))??;

    tokio::fs::create_dir_all("Music")
        .await
        .map_err(|e| format!("Failed to create Music dir: {e}"))?;

    let file_name = mp3_path
        .file_name()
        .ok_or("Downloaded mp3 path has no filename")?;
    let dest = std::path::Path::new("Music").join(file_name);

    move_file(&mp3_path, &dest)
        .await
        .map_err(|e| format!("Failed to move finished file into Music/: {e}"))?;

    Ok(())
}

/// The temp dir and Music/ may be on different filesystems (e.g. /tmp is
/// tmpfs), so a plain rename can fail with EXDEV — fall back to copy+delete.
async fn move_file(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    match tokio::fs::rename(src, dest).await {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) /* EXDEV */ => {
            tokio::fs::copy(src, dest).await?;
            tokio::fs::remove_file(src).await?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn write_tags(path: &std::path::Path, song_metadata: &metadata::SongMetadata) -> Result<(), String> {
    use lofty::prelude::*;
    use lofty::probe::Probe;

    let mut tagged_file = Probe::open(path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    if tagged_file.primary_tag().is_none() {
        let tag_type = tagged_file.primary_tag_type();
        tagged_file.insert_tag(lofty::tag::Tag::new(tag_type));
    }
    let tag = tagged_file
        .primary_tag_mut()
        .expect("tag was just inserted if missing");

    tag.set_title(song_metadata.title.clone());
    tag.set_artist(song_metadata.artist.clone());
    if let Some(album) = &song_metadata.album {
        tag.set_album(album.clone());
    }
    if let Some(track_number) = song_metadata.track_number {
        tag.set_track(track_number as u32);
    }
    if let Some(year) = song_metadata.year {
        tag.set_date(lofty::tag::items::Timestamp {
            year: year as u16,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
    }

    tagged_file
        .save_to_path(path, lofty::config::WriteOptions::default())
        .map_err(|e| e.to_string())
}

