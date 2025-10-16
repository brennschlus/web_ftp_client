use axum::{
    Router,
    routing::{get, post},
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};
use suppaftp::tokio::AsyncFtpStream;
use tokio::sync::Mutex;
use tower_http::{compression::CompressionLayer, services::ServeDir};

use crate::routes::{
    change_directory_handler, change_local_directory, connect_handler, disconnect_handler, events,
    index, list_handler, list_local,
};
mod filters;
mod helpers;
mod routes;
mod templates;

#[derive(Clone)]
pub struct AppState {
    pub connection: Arc<Mutex<Option<AsyncFtpStream>>>,
    pub local_path: Arc<Mutex<PathBuf>>,
}

#[derive(Deserialize)]
struct ConnectForm {
    host: String,
    port: u16,
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct ChangeDirectoryForm {
    directory: String,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        connection: Arc::new(Mutex::new(None)),
        local_path: Arc::new(Mutex::new(std::env::current_dir().unwrap())),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/connect", post(connect_handler))
        .route("/disconnect", post(disconnect_handler))
        .route("/list", get(list_handler))
        .route("/change_directory", post(change_directory_handler))
        .route("/local_list", get(list_local))
        .route("/local_change_directory", post(change_local_directory))
        .route("/events", get(events))
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(CompressionLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Сервер запущен на http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
