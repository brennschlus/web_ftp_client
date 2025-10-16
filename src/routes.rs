use std::{convert::Infallible, fs, str::FromStr, time::Duration};

use askama::Template;
use axum::{
    Form,
    extract::State,
    response::{Html, IntoResponse, Sse, sse::Event},
};
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use suppaftp::{list::File, tokio::AsyncFtpStream};
use tokio_stream::wrappers::IntervalStream;

use crate::{
    AppState, ChangeDirectoryForm, ConnectForm,
    helpers::is_connected,
    templates::{FilesTableTemplate, IndexTemplate, LocalFilesTableTemplate},
};

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
                    .map(|item| File::from_str(&item))
                    .into_iter()
                    .flatten()
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
            return Html(LocalFilesTableTemplate { files }.render().unwrap());
        }
        Err(err) => {
            return Html(format!("<li>Ошибка: {}</li>", err));
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
                    "<p>Подключено к серверу</p>"
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
        .flat_map(|events| futures_util::stream::iter(events).map(Ok));

    Sse::new(stream)
}

pub async fn connect_handler(
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

pub async fn disconnect_handler(State(state): State<AppState>) -> Html<String> {
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

        // if ftp.cwd(&form.directory).await.is_ok() {
        //     {
        //         let mut conn = state.connection.lock().await;
        //         *conn = Some(ftp);
        //     }

        //     Html("<div hx-get='/list' hx-trigger='load'></div>".to_string())
        // } else {
        //     Html("<p>Ошибка смены директории</p>".to_string())
        // }
    } else {
        Html("<p>Нет активного соединения</p>".to_string())
    }
}
