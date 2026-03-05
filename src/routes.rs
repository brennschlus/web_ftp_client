use std::{convert::Infallible, time::Duration};

use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Sse, sse::Event},
};
use axum_extra::extract::Form;
use ftp_fs::{FileSystem, FtpConnectParams, FtpFs};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::IntervalStream;

use crate::{
    error::AppError,
    state::AppState,
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

/// Helper to get a mutable reference to the active FtpFs connection.
async fn get_ftp<'a>(
    guard: &'a mut tokio::sync::MutexGuard<'_, Option<FtpFs>>,
) -> Result<&'a mut FtpFs, AppError> {
    guard.as_mut().ok_or(AppError::NotConnected)
}

/// Список файлов удалённой ФС (FTP).
pub async fn list_handler(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let mut ftp_guard = state.ftp.lock().await;
    let ftp = get_ftp(&mut ftp_guard).await?;
    let files = ftp.list().await?;
    let html = FilesTableTemplate { files }.render().unwrap();
    Ok(Html(html))
}

/// Список файлов локальной ФС.
pub async fn list_local(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let mut local = state.local_fs.lock().await;
    let files = local.list().await?;
    let html = LocalFilesTableTemplate { files }.render().unwrap();
    Ok(Html(html))
}

/// Смена директории в локальной ФС.
pub async fn change_local_directory(
    State(state): State<AppState>,
    Form(form): Form<ChangeDirectoryForm>,
) -> Result<Html<String>, AppError> {
    let mut local = state.local_fs.lock().await;
    if let Err(e) = local.change_dir(&form.directory).await {
        log::warn!("Ошибка смены локальной директории: {}", e);
    }
    Ok(Html(
        "<div hx-get='/local_list' hx-trigger='load'></div>".to_string(),
    ))
}

/// Подключение к FTP-серверу.
pub async fn connect_handler(
    State(state): State<AppState>,
    Form(form): Form<ConnectForm>,
) -> Result<Html<String>, AppError> {
    *state.connection_error.lock().await = None;

    let params = FtpConnectParams::new(form.host, form.port, form.username, form.password);

    match FtpFs::connect(params).await {
        Ok(ftp) => {
            *state.ftp.lock().await = Some(ftp);
            Ok(Html(
                r#"<div hx-get="/list" hx-trigger="load"></div>"#.to_string(),
            ))
        }
        Err(e) => {
            let msg = e.to_string();
            *state.connection_error.lock().await = Some(msg.clone());
            Ok(Html(format!("<p>{}</p>", msg)))
        }
    }
}

/// Отключение от FTP-сервера.
pub async fn disconnect_handler(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let ftp_opt = state.ftp.lock().await.take();
    *state.connection_error.lock().await = None;

    if let Some(ftp) = ftp_opt {
        ftp.disconnect().await.unwrap_or_else(|e| {
            log::warn!("Ошибка при отключении: {}", e);
        });
    }

    Ok(Html(
        "<ul id='remote-list'><li>Нет данных</li></ul>".to_string(),
    ))
}

/// Смена директории на FTP-сервере.
pub async fn change_directory_handler(
    State(state): State<AppState>,
    Form(form): Form<ChangeDirectoryForm>,
) -> Result<Html<String>, AppError> {
    let mut ftp_guard = state.ftp.lock().await;
    let ftp = get_ftp(&mut ftp_guard).await?;
    ftp.change_dir(&form.directory).await?;
    Ok(Html(
        "<div hx-get='/list' hx-trigger='load'></div>".to_string(),
    ))
}

/// Общая логика передачи файлов (скачивание или загрузка)
async fn handle_transfer(
    state: AppState,
    files: Vec<String>,
    is_upload: bool,
) -> Result<(), AppError> {
    if files.is_empty() {
        return Ok(());
    }

    let local_path = state.local_fs.lock().await.path().to_path_buf();
    let mut ftp_guard = state.ftp.lock().await;

    if let Some(ftp) = ftp_guard.as_mut() {
        let transfer_status = state.transfer_status.clone();

        // callback для обновления статуса
        let progress_cb = move |progress: ftp_fs::TransferProgress| {
            let status = transfer_status.clone();
            let action = if is_upload {
                "Загрузка"
            } else {
                "Скачивание"
            };
            let msg = format!("{}: {}", action, progress.filename);
            tokio::spawn(async move {
                *status.lock().await = Some(msg);
            });
        };

        if is_upload {
            ftp.upload(&local_path, &files, progress_cb).await?;
        } else {
            ftp.download(&local_path, &files, progress_cb).await?;
        }
    }

    *state.transfer_status.lock().await = None;
    Ok(())
}

/// Загрузка выбранных файлов на FTP-сервер.
pub async fn upload_handler(
    State(state): State<AppState>,
    Form(form): axum_extra::extract::Form<UploadForm>,
) -> axum::response::Response {
    let _ = handle_transfer(state, form.files, true).await;
    ([("HX-Trigger", "refreshRemote")], Html("".to_string())).into_response()
}

/// Скачивание выбранных файлов с FTP-сервера.
pub async fn download_handler(
    State(state): State<AppState>,
    Form(form): axum_extra::extract::Form<DownloadForm>,
) -> axum::response::Response {
    let _ = handle_transfer(state, form.files, false).await;
    ([("HX-Trigger", "refreshLocal")], Html("".to_string())).into_response()
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
