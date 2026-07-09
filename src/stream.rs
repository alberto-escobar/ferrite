use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use sqlx::SqlitePool;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use crate::db;

pub async fn stream_track(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let track = match db::get_track_by_id(&pool, id).await {
        Some(t) => t,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let file = match tokio::fs::File::open(&track.file_path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let file_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let (start, end) = if let Some(range) = headers.get("range") {
        let range_str = range.to_str().unwrap_or("bytes=0-");
        let range_str = range_str.trim_start_matches("bytes=");
        let parts: Vec<&str> = range_str.split('-').collect();
        let start = parts[0].parse::<u64>().unwrap_or(0);
        let end = parts
            .get(1)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(file_size - 1)
            .min(start + 512000);
        (start, end)
    } else {
        (0, file_size - 1)
    };

    let chunk_size = end - start + 1;

    let mut file = file;
    if let Err(_) = file.seek(std::io::SeekFrom::Start(start)).await {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let mut buffer = vec![0u8; chunk_size as usize];
    if let Err(_) = file.read_exact(&mut buffer).await {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let content_type = match std::path::Path::new(&track.file_path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        _ => "application/octet-stream",
    };

    let content_range = format!("bytes {}-{}/{}", start, end, file_size);

    (
        StatusCode::PARTIAL_CONTENT,
        [
            ("Content-Type", content_type),
            ("Content-Range", content_range.as_str()),
            ("Accept-Ranges", "bytes"),
            ("Content-Length", &chunk_size.to_string()),
        ],
        buffer,
    )
        .into_response()
}