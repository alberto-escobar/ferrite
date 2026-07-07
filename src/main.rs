mod db;
mod scanner;

use axum::{
    Router,
    routing::{get, post},
    response::Html,
    extract::State,
};
use db::Track;
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() {
    let pool = db::init_db("sqlite://ferrite.db").await;
    
    let music_dir = std::path::Path::new("./Music");
    let tracks = scanner::scan_directory(music_dir);

    for track in tracks {
        db::insert_track(
            &pool,
            &track.title,
            &track.artist,
            track.album.as_deref(),
            &track.file_path,
            track.duration,
            track.track_number,
        ).await;
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/api/tracks", get(get_tracks))
        .route("/api/tracks/dummy", post(add_dummy_track))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Ferrite running at http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<String> {
    let html = std::fs::read_to_string("frontend/index.html").unwrap_or_else(|_| "<h1>Could not load page</h1>".to_string());

    Html(html)
}

async fn get_tracks(State(pool): State<SqlitePool>) -> ([(&'static str, &'static str); 1], String) {
    let tracks = db::get_all_tracks(&pool).await;
    let body = format!("[{}]", tracks.iter().map(track_to_json).collect::<Vec<_>>().join(","));
    ([("Content-Type", "application/json")], body)
}

async fn add_dummy_track(State(pool): State<SqlitePool>) -> ([(&'static str, &'static str); 1], String) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let file_path = format!("dummy/track_{}.mp3", ts);

    db::insert_track(
        &pool,
        "Dummy Track",
        "Dummy Artist",
        Some("Dummy Album"),
        &file_path,
        Some(180),
        Some(1),
    )
    .await;

    let tracks = db::get_all_tracks(&pool).await;
    let track = tracks
        .into_iter()
        .find(|t| t.file_path == file_path)
        .expect("Failed to find inserted dummy track");

    ([("Content-Type", "application/json")], track_to_json(&track))
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn track_to_json(track: &Track) -> String {
    format!(
        "{{\"id\":{},\"title\":\"{}\",\"artist\":\"{}\",\"album\":{},\"file_path\":\"{}\",\"duration\":{},\"track_number\":{}}}",
        track.id,
        json_escape(&track.title),
        json_escape(&track.artist),
        track.album.as_deref().map(|a| format!("\"{}\"", json_escape(a))).unwrap_or_else(|| "null".to_string()),
        json_escape(&track.file_path),
        track.duration.map(|d| d.to_string()).unwrap_or_else(|| "null".to_string()),
        track.track_number.map(|n| n.to_string()).unwrap_or_else(|| "null".to_string()),
    )
}