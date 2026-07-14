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
    pub year: Option<i64>,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct Download {
    pub id: i64,
    pub url: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
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
        "SELECT id, title, artist, album, file_path, duration, track_number, year
         FROM tracks
         ORDER BY artist, album, track_number",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to query tracks")
}

pub async fn get_track_by_id(pool: &SqlitePool, id: i64) -> Option<Track> {
    sqlx::query_as::<_, Track>(
        "SELECT id, title, artist, album, file_path, duration, track_number, year
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
    year: Option<i64>,
) {
    sqlx::query(
        "INSERT OR IGNORE INTO tracks (title, artist, album, file_path, duration, track_number, year)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(title)
    .bind(artist)
    .bind(album)
    .bind(file_path)
    .bind(duration)
    .bind(track_number)
    .bind(year)
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

// ── Download queries ──────────────────────────────────────────────────────────

pub async fn insert_download(pool: &SqlitePool, url: &str) -> i64 {
    let result = sqlx::query("INSERT INTO downloads (url, status) VALUES (?, 'pending')")
        .bind(url)
        .execute(pool)
        .await
        .expect("Failed to insert download");

    result.last_insert_rowid()
}

pub async fn update_download_status(pool: &SqlitePool, id: i64, status: &str, error: Option<&str>) {
    sqlx::query("UPDATE downloads SET status = ?, error = ? WHERE id = ?")
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(pool)
        .await
        .expect("Failed to update download");
}

pub async fn get_all_downloads(pool: &SqlitePool) -> Vec<Download> {
    sqlx::query_as::<_, Download>(
        "SELECT id, url, status, error, created_at
         FROM downloads
         ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to query downloads")
}