use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use sqlx::SqlitePool;
use crate::db;
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

    // yt-dlp downloads the raw stream and does its ffmpeg audio-extraction pass
    // in place, so without this, half-finished files show up in Music/ while a
    // download is still running. Routing temp/working files to a scratch dir
    // keeps Music/ untouched until the finished mp3 is moved in.
    let temp_dir = std::env::temp_dir().join(format!("ferrite-yt-dlp-{id}"));

    let mut child = match Command::new("yt-dlp")
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg("--paths")
        .arg(format!("temp:{}", temp_dir.display()))
        .arg("--paths")
        .arg("home:Music")
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
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    match status {
        Ok(s) if s.success() => {
            db::update_download_status(&pool, id, "done", None).await;
            rescan_library(&pool).await;
        }
        Ok(_) => {
            let error = stderr_lines.join("\n");
            db::update_download_status(&pool, id, "failed", Some(&error)).await;
        }
        Err(e) => {
            db::update_download_status(&pool, id, "failed", Some(&e.to_string())).await;
        }
    }
}

async fn rescan_library(pool: &SqlitePool) {
    let music_dir = std::path::Path::new("./Music").to_path_buf();
    let tracks = tokio::task::spawn_blocking(move || crate::scanner::scan_directory(&music_dir))
        .await
        .unwrap_or_default();

    for track in tracks {
        db::insert_track(
            pool,
            &track.title,
            &track.artist,
            track.album.as_deref(),
            &track.file_path,
            track.duration,
            track.track_number,
        ).await;
    }
}