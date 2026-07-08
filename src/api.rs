use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use sqlx::SqlitePool;
use crate::db;

// ── Tracks ────────────────────────────────────────────────────────────────────

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

// ── Albums ────────────────────────────────────────────────────────────────────

pub async fn get_all_albums(
    State(pool): State<SqlitePool>,
) -> Json<Vec<String>> {
    let albums = db::get_all_albums(&pool).await;
    Json(albums)
}

// ── Artists ───────────────────────────────────────────────────────────────────

pub async fn get_all_artists(
    State(pool): State<SqlitePool>,
) -> Json<Vec<String>> {
    let artists = db::get_all_artists(&pool).await;
    Json(artists)
}