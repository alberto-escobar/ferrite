use axum::{
    Router,
    routing::get,
    response::Html,
};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(index));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Ferrite running at http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<String> {
    let html = std::fs::read_to_string("frontend/index.html").unwrap_or_else(|_| "<h1>Could not load page</h1>".to_string());

    Html(html)
}