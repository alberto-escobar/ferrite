use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use sqlx::SqlitePool;
use crate::db;
use std::process::Command;

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

pub async fn fetch_song(
    Json(body): Json<ImportRequest>,
) -> Json<serde_json::Value> {
    let status = Command::new("yt-dlp")
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg(&body.url)
        .status()
        .expect("Failed to execute command");

    if status.success() {
        Json(serde_json::json!({ "status": "ok" }))
    } else {
        Json(serde_json::json!({ "status": "error" }))
    }
}