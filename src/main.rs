mod db;
mod scanner;
mod api;
mod stream;

use axum::{
    Router,
    routing::get,
    response::Html,
};

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
        .route("/api/tracks", get(api::get_all_tracks))
        .route("/api/tracks/{id}", get(api::get_track_by_id))
        .route("/api/albums", get(api::get_all_albums))
        .route("/api/artists", get(api::get_all_artists))
        .route("/api/tracks/{id}/stream", get(stream::stream_track))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Ferrite running at http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<String> {
    let html = std::fs::read_to_string("frontend/index.html")
        .unwrap_or_else(|_| "<h1>Could not load page</h1>".to_string());
    Html(html)
}