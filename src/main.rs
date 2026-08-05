mod db;
mod scanner;
mod api;
mod metadata;
mod stream;

use axum::{
    Router,
    routing::{get, post},
    response::{Html, IntoResponse},
    extract::Path,
    http::{StatusCode, header},
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

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
            track.year,
        ).await;
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/static/{*path}", get(static_asset))
        .route("/api/tracks", get(api::get_all_tracks))
        .route("/api/tracks/{id}", get(api::get_track_by_id))
        .route("/api/albums", get(api::get_all_albums))
        .route("/api/artists", get(api::get_all_artists))
        .route("/api/tracks/{id}/stream", get(stream::stream_track))
        .route("/api/fetch", post(api::fetch_song))
        .route("/api/downloads", get(api::get_downloads))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Ferrite server running at http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<String> {
    let html = std::fs::read_to_string("frontend/index.html")
        .unwrap_or_else(|_| "<h1>Could not load page</h1>".to_string());
    Html(html)
}

async fn static_asset(Path(path): Path<String>) -> impl IntoResponse {
    if path.contains("..") {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    let content_type = if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else {
        "application/octet-stream"
    };

    match std::fs::read(format!("frontend/{path}")) {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type)], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}