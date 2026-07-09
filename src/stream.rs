use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use sqlx::SqlitePool;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use crate::db;

const MAX_CHUNK_SIZE: u64 = 512_000;

fn parse_byte_range(headers: &HeaderMap, file_size: u64) -> (u64, u64) {
    let Some(range) = headers.get("range") else {
        return (0, file_size - 1);
    };

    let range_str = range.to_str().unwrap_or("bytes=0-");
    let range_str = range_str.trim_start_matches("bytes=");
    let parts: Vec<&str> = range_str.split('-').collect();

    let start = parts[0].parse::<u64>().unwrap_or(0);
    let end = parts
        .get(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(file_size - 1)
        .min(start + MAX_CHUNK_SIZE);

    (start, end)
}

fn content_type_for_path(file_path: &str) -> &'static str {
    match std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        _ => "application/octet-stream",
    }
}

async fn read_chunk(
    file: &mut tokio::fs::File,
    start: u64,
    chunk_size: u64,
) -> std::io::Result<Vec<u8>> {
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut buffer = vec![0u8; chunk_size as usize];
    file.read_exact(&mut buffer).await?;
    Ok(buffer)
}

fn build_partial_content_response(
    content_type: &str,
    start: u64,
    end: u64,
    file_size: u64,
    buffer: Vec<u8>,
) -> impl IntoResponse {
    let content_range = format!("bytes {}-{}/{}", start, end, file_size);
    let content_length = buffer.len().to_string();

    (
        StatusCode::PARTIAL_CONTENT,
        [
            ("Content-Type", content_type.to_string()),
            ("Content-Range", content_range),
            ("Accept-Ranges", "bytes".to_string()),
            ("Content-Length", content_length),
        ],
        buffer,
    )
}

pub async fn stream_track(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let track = match db::get_track_by_id(&pool, id).await {
        Some(t) => t,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let mut file = match tokio::fs::File::open(&track.file_path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let file_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let (start, end) = parse_byte_range(&headers, file_size);
    let chunk_size = end - start + 1;

    let buffer = match read_chunk(&mut file, start, chunk_size).await {
        Ok(b) => b,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let content_type = content_type_for_path(&track.file_path);

    build_partial_content_response(content_type, start, end, file_size, buffer).into_response()
}
