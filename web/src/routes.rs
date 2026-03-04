use std::{convert::Infallible, time::Duration};

use askama::Template;
use axum_extra::extract::Form;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Sse, sse::Event},
};
use ftp_fs::{FtpConnectParams, FtpFs, FileSystem};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::IntervalStream;

use crate::{
    AppState,
    templates::{FilesTableTemplate, IndexTemplate, LocalFilesTableTemplate},
};

// ---------------------------------------------------------------------------
// Form types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ConnectForm {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ChangeDirectoryForm {
    pub directory: String,
}

#[derive(Deserialize)]
pub struct UploadForm {
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Deserialize)]
pub struct DownloadForm {
    #[serde(default)]
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn index() -> impl IntoResponse {
    Html(IndexTemplate {}.render().unwrap())
}

/// Список файлов удалённой ФС (FTP).
pub async fn list_handler(State(state): State<AppState>) -> Html<String> {
    let mut ftp_guard = state.ftp.lock().await;
    match ftp_guard.as_mut() {
        Some(ftp) => match ftp.list().await {
            Ok(files) => Html(FilesTableTemplate { files }.render().unwrap()),
            Err(e) => Html(format!("<li>Ошибка: {}</li>", e)),
        },
        None => Html("<li>Нет активного соединения</li>".into()),
    }
}

/// Список файлов локальной ФС.
pub async fn list_local(State(state): State<AppState>) -> Html<String> {
    let mut local = state.local_fs.lock().await;
    match local.list().await {
        Ok(files) => Html(LocalFilesTableTemplate { files }.render().unwrap()),
        Err(e) => Html(format!("<li>Ошибка: {}</li>", e)),
    }
}

/// Смена директории в локальной ФС.
pub async fn change_local_directory(
    State(state): State<AppState>,
    Form(form): Form<ChangeDirectoryForm>,
) -> Html<String> {
    let mut local = state.local_fs.lock().await;
    if let Err(e) = local.change_dir(&form.directory).await {
        log::warn!("Ошибка смены локальной директории: {}", e);
    }
    Html("<div hx-get='/local_list' hx-trigger='load'></div>".to_string())
}

/// Подключение к FTP-серверу.
pub async fn connect_handler(
    State(state): State<AppState>,
    Form(form): Form<ConnectForm>,
) -> Html<String> {
    *state.connection_error.lock().await = None;

    let params = FtpConnectParams::new(form.host, form.port, form.username, form.password);

    match FtpFs::connect(params).await {
        Ok(ftp) => {
            *state.ftp.lock().await = Some(ftp);
            Html(r#"<div hx-get="/list" hx-trigger="load"></div>"#.to_string())
        }
        Err(e) => {
            let msg = e.to_string();
            *state.connection_error.lock().await = Some(msg.clone());
            Html(format!("<p>{}</p>", msg))
        }
    }
}

/// Отключение от FTP-сервера.
pub async fn disconnect_handler(State(state): State<AppState>) -> Html<String> {
    let ftp_opt = state.ftp.lock().await.take();
    *state.connection_error.lock().await = None;

    if let Some(ftp) = ftp_opt {
        match ftp.disconnect().await {
            Ok(_) => Html("<ul id='remote-list'><li>Нет данных</li></ul>".to_string()),
            Err(e) => Html(format!("<p>Ошибка отключения: {}</p>", e)),
        }
    } else {
        Html("<p>Нет активного соединения</p>".to_string())
    }
}

/// Смена директории на FTP-сервере.
pub async fn change_directory_handler(
    State(state): State<AppState>,
    Form(form): Form<ChangeDirectoryForm>,
) -> Html<String> {
    let mut ftp_guard = state.ftp.lock().await;
    match ftp_guard.as_mut() {
        Some(ftp) => match ftp.change_dir(&form.directory).await {
            Ok(_) => Html("<div hx-get='/list' hx-trigger='load'></div>".to_string()),
            Err(e) => Html(format!("<p>Ошибка смены директории: {}</p>", e)),
        },
        None => Html("<p>Нет активного соединения</p>".to_string()),
    }
}

/// Загрузка выбранных файлов на FTP-сервер.
pub async fn upload_handler(
    State(state): State<AppState>,
    Form(form): axum_extra::extract::Form<UploadForm>,
) -> axum::response::Response {
    if form.files.is_empty() {
        return Html("".to_string()).into_response();
    }

    let local_path = state.local_fs.lock().await.path().to_path_buf();
    let mut ftp_guard = state.ftp.lock().await;

    if let Some(ftp) = ftp_guard.as_mut() {
        let transfer_status = state.transfer_status.clone();
        let _ = ftp
            .upload(&local_path, &form.files, |progress| {
                let status = transfer_status.clone();
                let msg = format!("Загрузка: {}", progress.filename);
                tokio::spawn(async move {
                    *status.lock().await = Some(msg);
                });
            })
            .await;
    }

    *state.transfer_status.lock().await = None;
    (
        [("HX-Trigger", "refreshRemote")],
        Html("".to_string()),
    )
    .into_response()
}

/// Скачивание выбранных файлов с FTP-сервера.
pub async fn download_handler(
    State(state): State<AppState>,
    Form(form): axum_extra::extract::Form<DownloadForm>,
) -> axum::response::Response {
    if form.files.is_empty() {
        return Html("".to_string()).into_response();
    }

    let local_path = state.local_fs.lock().await.path().to_path_buf();
    let mut ftp_guard = state.ftp.lock().await;

    if let Some(ftp) = ftp_guard.as_mut() {
        let transfer_status = state.transfer_status.clone();
        let _ = ftp
            .download(&local_path, &form.files, |progress| {
                let status = transfer_status.clone();
                let msg = format!("Скачивание: {}", progress.filename);
                tokio::spawn(async move {
                    *status.lock().await = Some(msg);
                });
            })
            .await;
    }

    *state.transfer_status.lock().await = None;
    (
        [("HX-Trigger", "refreshLocal")],
        Html("".to_string()),
    )
    .into_response()
}

/// SSE-поток статуса подключения и передачи.
pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let interval = tokio::time::interval(Duration::from_secs(2));

    let stream = IntervalStream::new(interval)
        .then(move |_| {
            let state = state.clone();
            async move {
                let connected = {
                    let mut ftp_guard = state.ftp.lock().await;
                    match ftp_guard.as_mut() {
                        Some(ftp) => ftp.ping().await,
                        None => false,
                    }
                };

                let footer_html = if connected {
                    let transfer = state.transfer_status.lock().await.clone();
                    if let Some(msg) = transfer {
                        format!("<p>🔄 {}</p>", msg)
                    } else {
                        "<p>Подключено к серверу</p>".to_string()
                    }
                } else {
                    let error = state.connection_error.lock().await.clone();
                    if let Some(err_msg) = error {
                        format!("<p>❌ Ошибка: {}</p>", err_msg)
                    } else {
                        "<p>❌ Нет подключения</p>".to_string()
                    }
                };

                let button_html = if connected {
                    r#"<button class="button" hx-post="/disconnect" hx-target='#remote-list' hx-swap="innerHTML">Отключиться</button>"#
                } else {
                    r#"<button class="button" hx-post="/connect" hx-target='#remote-list' hx-swap="innerHTML">Подключиться</button>"#
                };

                vec![
                    Event::default().event("footer").data(footer_html),
                    Event::default().event("button").data(button_html),
                ]
            }
        })
        .flat_map(|events| futures_util::stream::iter(events).map(Ok));

    Sse::new(stream)
}
