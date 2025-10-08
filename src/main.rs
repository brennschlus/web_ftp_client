use askama::Template;
use axum::{
    Form, Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;
use suppaftp::tokio::AsyncFtpStream;
use tokio::sync::Mutex;
use tower_http::{compression::CompressionLayer, services::ServeDir};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

#[derive(Clone)]
pub struct AppState {
    pub connection: Arc<Mutex<Option<AsyncFtpStream>>>,
}

#[derive(Deserialize)]
struct ConnectForm {
    host: String,
    port: u16,
    username: String,
    password: String,
}

async fn connect_handler(
    State(state): State<AppState>,
    Form(form): Form<ConnectForm>,
) -> Html<String> {
    let addr = format!("{}:{}", form.host, form.port);
    let ftp_stream = AsyncFtpStream::connect(&addr).await;

    let html = match ftp_stream {
        Ok(mut ftp) => {
            if ftp.login(&form.username, &form.password).await.is_ok() {
                *state.connection.lock().await = Some(ftp);
                // После успешного подключения просто возвращаем триггер на загрузку списка
                r#"<div hx-get="/list" hx-trigger="load"></div>"#.to_string()
            } else {
                "<p>Ошибка авторизации</p>".to_string()
            }
        }
        Err(e) => format!("<p>Ошибка подключения: {}</p>", e),
    };

    Html(html)
}

async fn list_handler(State(state): State<AppState>) -> Html<String> {
    let mut conn = state.connection.lock().await;
    if let Some(ref mut ftp) = *conn {
        match ftp.list(None).await {
            Ok(list) => {
                let html = list
                    .into_iter()
                    .map(|item| format!("<li>{}</li>", item))
                    .collect::<Vec<_>>()
                    .join("\n");
                Html(html)
            }
            Err(err) => Html(format!("<li>Ошибка: {}</li>", err)),
        }
    } else {
        Html("<li>Нет активного соединения</li>".into())
    }
}

#[tokio::main]
async fn main() {
    let state = AppState {
        connection: Arc::new(Mutex::new(None)),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/connect", post(connect_handler))
        .route("/list", get(list_handler))
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(CompressionLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🚀 http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> impl IntoResponse {
    Html(IndexTemplate {}.render().unwrap())
}
