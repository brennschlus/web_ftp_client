//! `ftp_fs` — библиотека для работы с локальной файловой системой и FTP.
//!
//! Предоставляет единый трейт [`FileSystem`] и конкретные реализации
//! [`LocalFs`] и [`FtpFs`], а также [`TransferEngine`] для передачи файлов.

pub mod error;
pub mod local;
pub mod ftp;
pub mod types;

pub use error::{FsError, FsResult};
pub use types::{FileEntry, FileKind, FtpConnectParams, TransferProgress};
pub use local::LocalFs;
pub use ftp::FtpFs;

use async_trait::async_trait;

/// Унифицированный асинхронный интерфейс для навигации по файловой системе.
///
/// Реализован как для локальной ФС ([`LocalFs`]), так и для FTP ([`FtpFs`]).
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Список файлов и директорий в текущей директории.
    async fn list(&mut self) -> FsResult<Vec<FileEntry>>;

    /// Перейти в директорию. Поддерживает `".."` для выхода на уровень выше.
    async fn change_dir(&mut self, path: &str) -> FsResult<()>;

    /// Текущий абсолютный путь (для отображения в UI).
    async fn current_dir(&self) -> FsResult<String>;
}
