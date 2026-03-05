use axum::response::{Html, IntoResponse, Response};
use ftp_fs::FsError;
use thiserror::Error;

/// Глобальная ошибка веб-приложения, оборачивающая внутренние и отдающая HTML.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Ошибка файловой системы: {0}")]
    Fs(#[from] FsError),

    #[error("Нет активного соединения с сервером")]
    NotConnected,
    // #[error("Внутренняя ошибка сервера: {0}")]
    // Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let msg = match self {
            Self::NotConnected => "<li>Нет активного соединения</li>".to_string(),
            _ => format!("<li>Ошибка: {}</li>", self),
        };
        Html(msg).into_response()
    }
}
