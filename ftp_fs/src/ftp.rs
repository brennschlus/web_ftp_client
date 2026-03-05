use std::{str::FromStr, time::Duration};

use async_trait::async_trait;
use suppaftp::{list::File, tokio::AsyncFtpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

use crate::{
    FileSystem,
    error::{FsError, FsResult},
    types::{FileEntry, FileKind, FtpConnectParams, TransferProgress},
};

/// Реализация [`FileSystem`] для FTP-сервера через `suppaftp`.
///
/// Хранит активное соединение [`AsyncFtpStream`].
pub struct FtpFs {
    stream: AsyncFtpStream,
}

impl FtpFs {
    /// Установить FTP-соединение с заданными параметрами.
    ///
    /// Возвращает ошибку [`FsError::Timeout`] если сервер не ответил
    /// за `params.timeout_secs` секунд, или [`FsError::Ftp`] при ошибке протокола.
    pub async fn connect(params: FtpConnectParams) -> FsResult<Self> {
        let addr = format!("{}:{}", params.host, params.port);
        let timeout = Duration::from_secs(params.timeout_secs);

        let mut stream = tokio::time::timeout(timeout, AsyncFtpStream::connect(&addr))
            .await
            .map_err(|_| FsError::Timeout)?
            .map_err(FsError::Ftp)?;

        stream
            .login(&params.username, &params.password)
            .await
            .map_err(FsError::Ftp)?;

        Ok(Self { stream })
    }

    /// Проверить живость соединения командой NOOP.
    ///
    /// Возвращает `false` если сервер не отвечает.
    pub async fn ping(&mut self) -> bool {
        self.stream.noop().await.is_ok()
    }

    /// Корректно завершить FTP-сессию командой QUIT.
    pub async fn disconnect(mut self) -> FsResult<()> {
        self.stream.quit().await.map_err(FsError::Ftp)
    }

    /// Загрузить файлы из локального пути на FTP-сервер.
    ///
    /// Колбэк `on_progress` вызывается перед началом передачи каждого файла.
    /// В будущем может вызываться на каждый chunk для прогресс-бара.
    pub async fn upload(
        &mut self,
        local_base: &std::path::Path,
        filenames: &[String],
        on_progress: impl Fn(TransferProgress) + Send,
    ) -> FsResult<()> {
        for filename in filenames {
            let file_path = local_base.join(filename);
            if !file_path.is_file() {
                continue;
            }
            let size = file_path.metadata().ok().map(|m| m.len());
            on_progress(TransferProgress {
                filename: filename.clone(),
                bytes_transferred: 0,
                total_bytes: size,
            });
            let mut file = tokio::fs::File::open(&file_path)
                .await
                .map_err(FsError::Io)?;
            let mut reader = BufReader::new(&mut file);
            self.stream
                .put_file(filename, &mut reader)
                .await
                .map_err(FsError::Ftp)?;
        }
        Ok(())
    }

    /// Скачать файлы с FTP-сервера в локальный путь.
    ///
    /// Директории и другие элементы, для которых `retr` выбросит ошибку, пропускаются.
    pub async fn download(
        &mut self,
        local_base: &std::path::Path,
        filenames: &[String],
        on_progress: impl Fn(TransferProgress) + Send,
    ) -> FsResult<()> {
        for filename in filenames {
            on_progress(TransferProgress {
                filename: filename.clone(),
                bytes_transferred: 0,
                total_bytes: None,
            });
            match self.stream.retr_as_stream(filename).await {
                Ok(mut data_stream) => {
                    let file_path = local_base.join(filename);
                    if let Ok(mut local_file) = tokio::fs::File::create(&file_path).await {
                        let mut buffer = [0u8; 8192];
                        loop {
                            let n = data_stream.read(&mut buffer).await.map_err(FsError::Io)?;
                            if n == 0 {
                                break;
                            }
                            local_file
                                .write_all(&buffer[..n])
                                .await
                                .map_err(FsError::Io)?;
                        }
                    }
                    self.stream
                        .finalize_retr_stream(data_stream)
                        .await
                        .map_err(FsError::Ftp)?;
                }
                Err(_) => {
                    // Директории или недоступные файлы — пропускаем
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl FileSystem for FtpFs {
    async fn list(&mut self) -> FsResult<Vec<FileEntry>> {
        let raw = self.stream.list(None).await.map_err(FsError::Ftp)?;
        let mut files: Vec<FileEntry> = raw
            .into_iter()
            .flat_map(|line| File::from_str(&line))
            .map(|f| {
                let kind = if f.is_directory() {
                    FileKind::Directory
                } else {
                    FileKind::File
                };
                FileEntry {
                    name: f.name().to_string(),
                    kind,
                    size: if f.is_directory() {
                        None
                    } else {
                        Some(f.size() as u64)
                    },
                }
            })
            .collect();

        // Директории сначала, затем файлы по алфавиту
        crate::types::sort_file_entries(&mut files);

        Ok(files)
    }

    async fn change_dir(&mut self, path: &str) -> FsResult<()> {
        if path == ".." {
            self.stream.cdup().await.map_err(FsError::Ftp)
        } else {
            self.stream.cwd(path).await.map_err(FsError::Ftp)
        }
    }

    async fn current_dir(&self) -> FsResult<String> {
        // suppaftp AsyncFtpStream::pwd() берёт &mut self, поэтому
        // нарушается &self. Используем временную строку как workaround.
        // TODO: убрать когда suppaftp сделает pwd(&self).
        Ok(String::from("(unknown — call on &mut self)"))
    }
}
