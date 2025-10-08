use askama::Template;
use axum::{
    Form, Router,
    extract::State,
    response::{Html, IntoResponse, Sse, sse::Event},
    routing::{get, post},
};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use suppaftp::tokio::AsyncFtpStream;
use tokio::sync::Mutex;
use tokio_stream::wrappers::IntervalStream;
use tower_http::{compression::CompressionLayer, services::ServeDir};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

#[derive(Template)]
#[template(path = "footer.html")]
struct FooterTemplate {
    footer: String,
}

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
                r#"<div hx-get="/list" hx-trigger="load"></div>"#.to_string()
            } else {
                "<p>Ошибка авторизации</p>".to_string()
            }
        }
        Err(e) => format!("<p>Ошибка подключения: {}</p>", e),
    };

    Html(html)
}

async fn disconnect_handler(State(state): State<AppState>) -> Html<String> {
    let mut conn = state.connection.lock().await;

    if let Some(mut ftp) = conn.take() {
        if ftp.quit().await.is_ok() {
            Html("<ul id='remote-list'><li>Нет данных</li></ul>".to_string())
        } else {
            Html("<p>Ошибка отключения</p>".to_string())
        }
    } else {
        Html("<p>Ошибка отключения</p>".to_string())
    }
}

async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("Старт отправки событий");

    let interval = tokio::time::interval(Duration::from_secs(2));

    // создаём поток
    let stream = IntervalStream::new(interval)
        .then(move |_| {
            let state = state.clone();
            async move {
                let connected = is_connected(&mut *state.connection.lock().await).await;

                let footer_html = if connected {
                    "<p>✅ Подключено к серверу</p>"
                } else {
                    "<p>❌ Нет подключения</p>"
                };

                let button_html = if connected {
                    r#"<button class="button" hx-post="/disconnect" hx-target='#remote-list' hx-swap="innerHTML">Отключиться</button>"#
                } else {
                    r#"<button class="button" hx-post="/connect" hx-target='#remote-list' hx-swap="innerHTML">Подключиться</button>"#
                };

                // возвращаем вектор событий
                vec![
                    Event::default().event("footer").data(footer_html),
                    Event::default().event("button").data(button_html),
                ]
            }
        })
        // разворачиваем вектор в отдельные Event'ы
        .flat_map(|events| futures_util::stream::iter(events).map(Ok));

    Sse::new(stream)
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

async fn footer_handler(State(state): State<AppState>) -> impl IntoResponse {
    let connected = is_connected(&mut *state.connection.lock().await).await;
    let footer = if connected {
        "Соединение активно".to_string()
    } else {
        "Нет активного соединения".to_string()
    };

    Html(FooterTemplate { footer }.render().unwrap())
}

async fn is_connected(state: &mut Option<AsyncFtpStream>) -> bool {
    if let Some(ftp) = state {
        match ftp.noop().await {
            Ok(_) => true,
            Err(_) => {
                *state = None;
                false
            }
        }
    } else {
        false
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
        .route("/disconnect", post(disconnect_handler))
        .route("/list", get(list_handler))
        .route("/footer", get(footer_handler))
        .route("/events", get(events))
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(CompressionLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Сервер запущен на http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> impl IntoResponse {
    Html(IndexTemplate {}.render().unwrap())
}
