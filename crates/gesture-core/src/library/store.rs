//! Хранилища словаря: в памяти и в JSON-файле.

use std::path::{Path, PathBuf};

use super::{CustomSign, LibraryError};

/// Хранилище словаря. Позволяет подставить файловое или сетевое хранилище.
///
/// Трейт требует `Send + Sync`, потому что словарь принадлежит общему
/// состоянию окна: к нему обращается и поток кадров, и поток интерфейса.
pub trait SignStore: Send + Sync {
    fn load(&self) -> Result<Vec<CustomSign>, LibraryError>;
    fn save(&self, signs: &[CustomSign]) -> Result<(), LibraryError>;
}

/// Хранилище в памяти: используется в тестах и как запасной вариант,
/// если файл недоступен.
#[derive(Debug, Default)]
pub struct MemoryStore {
    signs: std::sync::Mutex<Vec<CustomSign>>,
}

impl MemoryStore {
    pub fn new(signs: Vec<CustomSign>) -> Self {
        MemoryStore {
            signs: std::sync::Mutex::new(signs),
        }
    }
}

impl SignStore for MemoryStore {
    fn load(&self) -> Result<Vec<CustomSign>, LibraryError> {
        // Отравленный мьютекс означает панику в holders-потоке. Данные
        // словаря к этому моменту согласованы, поэтому читаем их как есть.
        Ok(self.signs.lock().map(|g| g.clone()).unwrap_or_default())
    }

    fn save(&self, signs: &[CustomSign]) -> Result<(), LibraryError> {
        let mut guard = self.signs.lock().map_err(|_| LibraryError::Lock)?;
        *guard = signs.to_vec();
        Ok(())
    }
}

/// Хранилище в JSON-файле — замена документов из iOS-версии.
#[derive(Debug)]
pub struct JsonFileStore {
    path: PathBuf,
}

impl JsonFileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        JsonFileStore { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SignStore for JsonFileStore {
    fn load(&self) -> Result<Vec<CustomSign>, LibraryError> {
        // Отсутствующий файл — это пустой словарь, а не ошибка:
        // приложение обязано запускаться при первом запуске.
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path).map_err(|e| LibraryError::Io {
            path: self.path.clone(),
            source: e,
        })?;
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&text).map_err(|e| LibraryError::Format(e.to_string()))
    }

    fn save(&self, signs: &[CustomSign]) -> Result<(), LibraryError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| LibraryError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }
        let text =
            serde_json::to_string_pretty(signs).map_err(|e| LibraryError::Format(e.to_string()))?;
        // Запись через временный файл: при сбое питания старый словарь
        // останется целым.
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|e| LibraryError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|e| LibraryError::Io {
            path: self.path.clone(),
            source: e,
        })?;
        Ok(())
    }
}
