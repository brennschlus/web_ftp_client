use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::{
    FileSystem,
    error::FsError,
    error::FsResult,
    types::{FileEntry, FileKind},
};

/// Реализация [`FileSystem`] для локальной файловой системы.
///
/// Хранит текущий путь навигации и реализует листинг, смену директории.
pub struct LocalFs {
    current_path: PathBuf,
}

impl LocalFs {
    /// Создать `LocalFs` с начальным путём.
    pub fn new(path: PathBuf) -> Self {
        Self { current_path: path }
    }

    /// Текущий путь как `&Path`.
    pub fn path(&self) -> &Path {
        &self.current_path
    }
}

#[async_trait]
impl FileSystem for LocalFs {
    async fn list(&mut self) -> FsResult<Vec<FileEntry>> {
        let entries = std::fs::read_dir(&self.current_path).map_err(FsError::Io)?;

        let mut files = Vec::new();
        for entry in entries.flatten() {
            let meta = entry.metadata().ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let (kind, size) = match &meta {
                Some(m) if m.is_dir() => (FileKind::Directory, None),
                Some(m) if m.is_symlink() => (FileKind::Symlink, None),
                Some(m) => (FileKind::File, Some(m.len())),
                None => (FileKind::File, None),
            };
            files.push(FileEntry { name, kind, size });
        }

        // Сортировка: директории сначала, затем файлы по алфавиту
        crate::types::sort_file_entries(&mut files);

        Ok(files)
    }

    async fn change_dir(&mut self, path: &str) -> FsResult<()> {
        let new_path = if path == ".." {
            self.current_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.current_path.clone())
        } else {
            self.current_path.join(path)
        };

        if new_path.is_dir() {
            self.current_path = new_path;
            Ok(())
        } else {
            Err(FsError::PathNotFound(path.to_string()))
        }
    }

    async fn current_dir(&self) -> FsResult<String> {
        Ok(self.current_path.to_string_lossy().to_string())
    }
}
