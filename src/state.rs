use std::sync::Arc;

use ftp_fs::LocalFs;
use tokio::sync::Mutex;

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
