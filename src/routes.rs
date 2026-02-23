use std::{convert::Infallible, fs, str::FromStr, time::Duration};

use askama::Template;
use axum_extra::extract::Form;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Sse, sse::Event},
};
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use suppaftp::{list::File, tokio::AsyncFtpStream};
use tokio_stream::wrappers::IntervalStream;

use crate::{
    AppState, ChangeDirectoryForm, ConnectForm,
    helpers::is_connected,
    templates::{FilesTableTemplate, IndexTemplate, LocalFilesTableTemplate},
};

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

#[derive(Serialize)]
pub struct FileInfo {
    pub name: String,
    pub kind: String,
    pub size: String,
}

impl From<&File> for FileInfo {
    fn from(f: &File) -> Self {
        Self {
            name: f.name().to_string(),
            kind: match f.is_directory() {
                true => "dir".into(),
                false => "file".into(),
            },
            size: crate::filters::format_size(&f.size()).unwrap_or("unknown".into()),
        }
    }
}

pub async fn index() -> impl IntoResponse {
    Html(IndexTemplate {}.render().unwrap())
}

pub async fn list_handler(State(state): State<AppState>) -> Html<String> {
    let mut conn = state.connection.lock().await;
    if let Some(ref mut ftp) = *conn {
        match ftp.list(None).await {
            Ok(list) => {
                let files = list
                    .into_iter()
                    .flat_map(|item| File::from_str(&item))
                    .map(|item| FileInfo::from(&item))
                    .collect::<Vec<_>>();
                Html(FilesTableTemplate { files }.render().unwrap())
            }
            Err(err) => Html(format!("<li>Ошибка: {}</li>", err)),
        }
    } else {
        Html("<li>Нет активного соединения</li>".into())
    }
}

pub async fn list_local(State(state): State<AppState>) -> Html<String> {
    let path = state.local_path.lock().await.clone();

    let mut files = Vec::new();

    match fs::read_dir(&path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let meta = entry.metadata().ok();
                let name = entry.file_name().to_string_lossy().to_string();
                let kind = if meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
                    "dir"
                } else {
                    "file"
                }
                .to_string();
                let size: Option<u64> =
                    meta.and_then(|m| if m.is_file() { Some(m.len()) } else { None });
                files.push(FileInfo {
                    name,
                    kind,
                    size: crate::filters::format_size(&size.map(|s| s as usize).unwrap_or(0))
                        .unwrap_or("unknown".into()),
                });
            }
            Html(LocalFilesTableTemplate { files }.render().unwrap())
        }
        Err(err) => {
            Html(format!("<li>Ошибка: {}</li>", err))
        }
    }
}

pub async fn change_local_directory(
    State(state): State<AppState>,
    Form(form): Form<ChangeDirectoryForm>,
) -> Html<String> {
    let mut path = state.local_path.lock().await;

    let new_path = if form.directory == ".." {
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(path.clone())
    } else {
        path.join(&form.directory)
    };

    if new_path.is_dir() {
        *path = new_path;
    }

    Html("<div hx-get='/local_list' hx-trigger='load'></div>".to_string())
}

pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("Старт отправки событий");

    let interval = tokio::time::interval(Duration::from_secs(2));

    let stream = IntervalStream::new(interval)
        .then(move |_| {
            let state = state.clone();
            async move {
                let connected = is_connected(&mut *state.connection.lock().await).await;
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

                // возвращаем вектор событий
                vec![
                    Event::default().event("footer").data(footer_html),
                    Event::default().event("button").data(button_html),
                ]
            }
        })
        .flat_map(|events| futures_util::stream::iter(events).map(Ok));

    Sse::new(stream)
}

pub async fn connect_handler(
    State(state): State<AppState>,
    Form(form): Form<ConnectForm>,
) -> Html<String> {
    let addr = format!("{}:{}", form.host, form.port);
    
    // Очищаем предыдущую ошибку
    *state.connection_error.lock().await = None;

    let html = match tokio::time::timeout(Duration::from_secs(5), AsyncFtpStream::connect(&addr)).await {
        Ok(ftp_stream) => match ftp_stream {
            Ok(mut ftp) => {
                if ftp.login(&form.username, &form.password).await.is_ok() {
                    *state.connection.lock().await = Some(ftp);
                    r#"<div hx-get="/list" hx-trigger="load"></div>"#.to_string()
                } else {
                    let error_msg = "Ошибка авторизации".to_string();
                    *state.connection_error.lock().await = Some(error_msg.clone());
                    "<p>Ошибка авторизации</p>".to_string()
                }
            }
            Err(e) => {
                let error_msg = format!("Ошибка подключения: {}", e);
                *state.connection_error.lock().await = Some(error_msg.clone());
                format!("<p>{}</p>", error_msg)
            }
        },
        Err(_) => {
            let error_msg = "Таймаут подключения (5 секунд)".to_string();
            *state.connection_error.lock().await = Some(error_msg.clone());
            format!("<p>{}</p>", error_msg)
        }
    };

    Html(html)
}

pub async fn disconnect_handler(State(state): State<AppState>) -> Html<String> {
    let mut conn = state.connection.lock().await;

    if let Some(mut ftp) = conn.take() {
        *state.connection_error.lock().await = None;
        if ftp.quit().await.is_ok() {
            Html("<ul id='remote-list'><li>Нет данных</li></ul>".to_string())
        } else {
            Html("<p>Ошибка отключения</p>".to_string())
        }
    } else {
        Html("<p>Ошибка отключения</p>".to_string())
    }
}

pub async fn change_directory_handler(
    State(state): State<AppState>,
    Form(form): Form<ChangeDirectoryForm>,
) -> Html<String> {
    // Забираем FTP-соединение из состояния
    let ftp_opt = {
        let mut conn = state.connection.lock().await;
        conn.take()
    };

    if let Some(mut ftp) = ftp_opt {
        if &form.directory == ".." {
            match ftp.cdup().await {
                Ok(_) => {
                    {
                        let mut conn = state.connection.lock().await;
                        *conn = Some(ftp);
                    }

                    Html("<div hx-get='/list' hx-trigger='load'></div>".to_string())
                }

                Err(e) => {
                    println!("Ошибка смены директории: {}", e);
                    Html(format!("<p>Ошибка смены директории: {}</p>", e))
                }
            }
        } else {
            match ftp.cwd(&form.directory).await {
                Ok(_) => {
                    {
                        let mut conn = state.connection.lock().await;
                        *conn = Some(ftp);
                    }

                    Html("<div hx-get='/list' hx-trigger='load'></div>".to_string())
                }

                Err(e) => {
                    println!("Ошибка смены директории: {}", e);
                    Html(format!("<p>Ошибка смены директории: {}</p>", e))
                }
            }
        }
    } else {
        Html("<p>Нет активного соединения</p>".to_string())
    }
}

pub async fn upload_handler(
    State(state): State<AppState>,
    Form(form): axum_extra::extract::Form<UploadForm>,
) -> axum::response::Response {
    if form.files.is_empty() {
        return Html("".to_string()).into_response();
    }

    let local_path = state.local_path.lock().await.clone();
    let ftp_opt = { state.connection.lock().await.take() };

    if let Some(mut ftp) = ftp_opt {
        for filename in form.files {
            let file_path = local_path.join(&filename);
            if file_path.is_file() {
                *state.transfer_status.lock().await = Some(format!("Загрузка: {}", filename));
                if let Ok(mut file) = tokio::fs::File::open(&file_path).await {
                    let mut reader = tokio::io::BufReader::new(&mut file);
                    let _ = ftp.put_file(&filename, &mut reader).await;
                }
            }
        }
        *state.transfer_status.lock().await = None;
        let mut conn = state.connection.lock().await;
        *conn = Some(ftp);
    }
    (
        [("HX-Trigger", "refreshRemote")],
        Html("".to_string())
    ).into_response()
}

pub async fn download_handler(
    State(state): State<AppState>,
    Form(form): axum_extra::extract::Form<DownloadForm>,
) -> axum::response::Response {
    if form.files.is_empty() {
        return Html("".to_string()).into_response();
    }

    let local_path = state.local_path.lock().await.clone();
    let ftp_opt = { state.connection.lock().await.take() };

    if let Some(mut ftp) = ftp_opt {
        for filename in form.files {
            *state.transfer_status.lock().await = Some(format!("Скачивание: {}", filename));
            
            let stream = ftp.retr_as_stream(&filename).await;
            if let Ok(mut data_stream) = stream {
                let file_path = local_path.join(&filename);
                if let Ok(mut local_file) = tokio::fs::File::create(file_path).await {
                    use tokio::io::AsyncReadExt;
                    let mut buffer = [0; 8192];
                    while let Ok(n) = data_stream.read(&mut buffer).await {
                        if n == 0 {
                            break;
                        }
                        let _ = tokio::io::AsyncWriteExt::write_all(&mut local_file, &buffer[..n]).await;
                    }
                }
                ftp.finalize_retr_stream(data_stream).await.ok();
            } else {
               // Ignore folder error
            }
        }
        *state.transfer_status.lock().await = None;
        let mut conn = state.connection.lock().await;
        *conn = Some(ftp);
    }
    (
        [("HX-Trigger", "refreshLocal")],
        Html("".to_string())
    ).into_response()
}
