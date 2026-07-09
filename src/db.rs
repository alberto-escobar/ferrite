// Used AI here as this is a DB helper and will have a lot of repeated code
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::str::FromStr;

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub file_path: String,
    pub duration: Option<i64>,
    pub track_number: Option<i64>,
}

// ── Setup ─────────────────────────────────────────────────────────────────────

pub async fn init_db(database_url: &str) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str(database_url)
                .expect("Invalid database URL")
                .create_if_missing(true),
        )
        .await
        .expect("Failed to connect to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

// ── Track queries ─────────────────────────────────────────────────────────────

pub async fn get_all_tracks(pool: &SqlitePool) -> Vec<Track> {
    sqlx::query_as::<_, Track>(
        "SELECT id, title, artist, album, file_path, duration, track_number
         FROM tracks
         ORDER BY artist, album, track_number",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to query tracks")
}

pub async fn get_track_by_id(pool: &SqlitePool, id: i64) -> Option<Track> {
    sqlx::query_as::<_, Track>(
        "SELECT id, title, artist, album, file_path, duration, track_number
         FROM tracks WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("Failed to query track")
}

pub async fn insert_track(
    pool: &SqlitePool,
    title: &str,
    artist: &str,
    album: Option<&str>,
    file_path: &str,
    duration: Option<i64>,
    track_number: Option<i64>,
) {
    sqlx::query(
        "INSERT OR IGNORE INTO tracks (title, artist, album, file_path, duration, track_number)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(title)
    .bind(artist)
    .bind(album)
    .bind(file_path)
    .bind(duration)
    .bind(track_number)
    .execute(pool)
    .await
    .expect("Failed to insert track");
}

// ── Album queries ─────────────────────────────────────────────────────────────

pub async fn get_all_albums(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT album FROM tracks
         WHERE album IS NOT NULL
         ORDER BY album",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to query albums")
}

// ── Artist queries ────────────────────────────────────────────────────────────

pub async fn get_all_artists(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT artist FROM tracks
         ORDER BY artist",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to query artists")
}