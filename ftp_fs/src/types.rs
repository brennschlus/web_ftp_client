use serde::{Deserialize, Serialize};

/// Тип записи файловой системы.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

/// Единое представление файла или директории для обеих файловых систем.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Имя файла или директории (без пути).
    pub name: String,
    /// Тип: файл, директория или символическая ссылка.
    pub kind: FileKind,
    /// Размер в байтах. `None` для директорий.
    pub size: Option<u64>,
}

impl FileEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == FileKind::Directory
    }

    pub fn is_file(&self) -> bool {
        self.kind == FileKind::File
    }

    /// Человекочитаемый размер файла для отображения в UI.
    /// Возвращает `"—"` для директорий или если размер неизвестен.
    pub fn size_display(&self) -> String {
        match self.size {
            None => "—".to_string(),
            Some(s) => {
                let s = s as f64;
                if s < 1024.0 {
                    format!("{:.0} B", s)
                } else if s < 1024.0 * 1024.0 {
                    format!("{:.1} KB", s / 1024.0)
                } else if s < 1024.0 * 1024.0 * 1024.0 {
                    format!("{:.1} MB", s / 1024.0 / 1024.0)
                } else {
                    format!("{:.1} GB", s / 1024.0 / 1024.0 / 1024.0)
                }
            }
        }
    }
}

/// Параметры для подключения к FTP-серверу.
#[derive(Debug, Clone)]
pub struct FtpConnectParams {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// Таймаут подключения в секундах. По умолчанию: 5.
    pub timeout_secs: u64,
}

impl FtpConnectParams {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            password: password.into(),
            timeout_secs: 5,
        }
    }
}

/// Прогресс передачи файла — для обратных вызовов и SSE-событий.
#[derive(Debug, Clone)]
pub struct TransferProgress {
    /// Имя передаваемого файла.
    pub filename: String,
    /// Количество уже переданных байт.
    pub bytes_transferred: u64,
    /// Общий размер файла, если известен.
    pub total_bytes: Option<u64>,
}
