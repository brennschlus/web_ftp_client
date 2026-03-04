use axum::{
    Router,
    routing::{get, post},
};
use ftp_fs::LocalFs;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::{compression::CompressionLayer, services::ServeDir};

mod routes;
mod templates;

/// Глобальное состояние приложения.
///
/// Разделяется между всеми обработчиками запросов через [`Arc`].
#[derive(Clone)]
pub struct AppState {
    /// Активное FTP-соединение (если есть).
    pub ftp: Arc<Mutex<Option<ftp_fs::FtpFs>>>,
    /// Сообщение об ошибке последнего подключения.
    pub connection_error: Arc<Mutex<Option<String>>>,
    /// Навигация по локальной ФС.
    pub local_fs: Arc<Mutex<LocalFs>>,
    /// Статус текущей передачи файлов (для SSE).
    pub transfer_status: Arc<Mutex<Option<String>>>,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        ftp: Arc::new(Mutex::new(None)),
        connection_error: Arc::new(Mutex::new(None)),
        local_fs: Arc::new(Mutex::new(LocalFs::new(std::env::current_dir().unwrap()))),
        transfer_status: Arc::new(Mutex::new(None)),
    };

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/connect", post(routes::connect_handler))
        .route("/disconnect", post(routes::disconnect_handler))
        .route("/list", get(routes::list_handler))
        .route("/change_directory", post(routes::change_directory_handler))
        .route("/local_list", get(routes::list_local))
        .route(
            "/local_change_directory",
            post(routes::change_local_directory),
        )
        .route("/upload", post(routes::upload_handler))
        .route("/download", post(routes::download_handler))
        .route("/events", get(routes::events))
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(CompressionLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Сервер запущен на http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
