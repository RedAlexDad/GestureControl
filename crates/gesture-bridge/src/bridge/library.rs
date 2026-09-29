//! Работа со словарём: запись, удаление, сохранение.

use gesture_core::JsonFileStore;

use crate::error::BridgeError;
use crate::state::{AppState, SignRow};

use super::GestureBridge;

impl GestureBridge {
    /// Начинает запись нового жеста.
    pub fn start_recording(&mut self, word: &str, is_dynamic: bool) -> AppState {
        let time = self.frame_time();
        self.engine.begin_recording(word, is_dynamic, time);
        self.state()
    }

    /// Отменяет запись жеста.
    pub fn cancel_recording(&mut self) -> AppState {
        self.engine.cancel_recording();
        self.state()
    }

    /// Удаляет жест по идентификатору.
    ///
    /// Идентификатор, а не позиция: после удаления строки все позиции
    /// сдвигаются, и нажатие второй кнопки удалило бы не тот жест.
    pub fn delete_sign(&mut self, id: &str) -> AppState {
        self.engine.delete_sign_id(id);
        self.state()
    }

    /// Жесты словаря для списка в интерфейсе.
    pub fn signs(&self) -> Vec<SignRow> {
        self.engine.signs().iter().map(SignRow::from).collect()
    }

    /// Сбрасывает словарь и сохраняет пустой.
    pub fn clear_signs(&mut self) -> Result<AppState, BridgeError> {
        while !self.engine.signs().is_empty() {
            self.engine.delete_sign(0);
        }
        self.save()?;
        Ok(self.state())
    }

    /// Принудительно записывает словарь на диск.
    ///
    /// Движок сохраняет сам после записи и удаления, поэтому это нужно
    /// только при снятии с диска: интерфейсу «сохранить» без всякой причины.
    pub fn save(&self) -> Result<(), BridgeError> {
        let Some(path) = &self.store_path else {
            return Err(BridgeError::Unavailable("путь к словарю не задан"));
        };
        self.engine
            .library()
            .save(&JsonFileStore::new(path.as_path()))
            .map_err(BridgeError::from)
    }
}
