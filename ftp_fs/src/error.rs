use thiserror::Error;

/// Единый тип ошибки для всех операций `ftp_fs`.
#[derive(Debug, Error)]
pub enum FsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("FTP error: {0}")]
    Ftp(#[from] suppaftp::FtpError),

    #[error("Connection timeout")]
    Timeout,

    #[error("Not connected to any FTP server")]
    NotConnected,

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Псевдоним результата с [`FsError`].
pub type FsResult<T> = Result<T, FsError>;
