//! Словарь жестов пользователя: обучение, поиск и хранение.
//!
//! Перенос `CustomSign` и `SignLibrary` из `SignLibrary.swift`.
//!
//! Статичные жесты ищутся ближайшим соседом (k-NN, k=3) в пространстве
//! признаков позы. Динамические жесты сравниваются шаблонами по subsequence
//! DTW, после чего кандидаты ранжируются числом совпадений подряд.

mod classify;
mod sign;
mod store;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use uuid::Uuid;

use crate::motion::MotionFeatures;

pub use sign::CustomSign;
pub use store::{JsonFileStore, MemoryStore, SignStore};

/// Словарь пользовательских жестов.
#[derive(Debug)]
pub struct SignLibrary {
    signs: Vec<CustomSign>,
    /// Идемпотентность: готовые шаблоны кешируются, чтобы не пересчитывать
    /// обрезку и прореживание на каждом кадре.
    prepared: BTreeMap<Uuid, Vec<Vec<Vec<f32>>>>,
}

impl SignLibrary {
    /// Загружает словарь из хранилища. Ошибка чтения не блокирует запуск:
    /// словарь остаётся пустым, а данные не перезаписываются до явного
    /// сохранения пользователем.
    pub fn load(store: &dyn SignStore) -> Self {
        let signs = store.load().unwrap_or_default();
        let prepared = signs
            .iter()
            .map(|s| (s.id, s.prepared_templates()))
            .collect();
        SignLibrary { signs, prepared }
    }

    pub fn from_signs(signs: Vec<CustomSign>) -> Self {
        let prepared = signs
            .iter()
            .map(|s| (s.id, s.prepared_templates()))
            .collect();
        SignLibrary { signs, prepared }
    }

    pub fn signs(&self) -> &[CustomSign] {
        &self.signs
    }

    pub fn is_empty(&self) -> bool {
        self.signs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.signs.len()
    }

    /// Полное описание словаря для интерфейса.
    pub fn snapshot(&self) -> Vec<CustomSign> {
        self.signs.clone()
    }

    /// Ищет жест по индексу. `None` — индекс вне диапазона.
    pub fn get(&self, index: usize) -> Option<&CustomSign> {
        self.signs.get(index)
    }

    pub fn index_of_id(&self, id: Uuid) -> Option<usize> {
        self.signs.iter().position(|s| s.id == id)
    }

    /// Добавляет пример жеста. Слово сравнивается без учёта регистра,
    /// поэтому «Привет» и «привет» — один и тот же жест.
    pub fn add(&mut self, word: &str, samples: Vec<Vec<f32>>) -> Uuid {
        match self.index_by_word(word) {
            Some(index) => {
                self.signs[index].samples.extend(samples);
                self.signs[index].id
            }
            None => {
                let sign = CustomSign {
                    id: Uuid::new_v4(),
                    word: word.to_string(),
                    samples,
                    sequences: Vec::new(),
                };
                self.signs.push(sign.clone());
                self.prepared.insert(sign.id, Vec::new());
                sign.id
            }
        }
    }

    /// Добавляет пример динамического жеста.
    ///
    /// Готовый шаблон пополняет кеш: пересчитывать обрезку и прореживание
    /// на каждом кадре незачем.
    pub fn add_sequence(&mut self, word: &str, sequence: Vec<Vec<f32>>) -> Uuid {
        match self.index_by_word(word) {
            Some(index) => {
                let id = self.signs[index].id;
                let template = MotionFeatures::prepare_template(&sequence);
                self.signs[index].sequences.push(sequence);
                self.prepared.entry(id).or_default().push(template);
                id
            }
            None => {
                let sign = CustomSign {
                    id: Uuid::new_v4(),
                    word: word.to_string(),
                    samples: Vec::new(),
                    sequences: vec![sequence.clone()],
                };
                let template = MotionFeatures::prepare_template(&sequence);
                self.signs.push(sign.clone());
                self.prepared.insert(sign.id, vec![template]);
                sign.id
            }
        }
    }

    /// Удаляет жест целиком.
    pub fn delete(&mut self, index: usize) -> Option<CustomSign> {
        if index >= self.signs.len() {
            return None;
        }
        let removed = self.signs.remove(index);
        self.prepared.remove(&removed.id);
        Some(removed)
    }

    /// Удаляет жест по идентификатору.
    pub fn delete_id(&mut self, id: Uuid) -> Option<CustomSign> {
        let index = self.index_of_id(id)?;
        self.delete(index)
    }

    /// Очищает словарь.
    pub fn clear(&mut self) {
        self.signs.clear();
        self.prepared.clear();
    }

    /// Сохраняет словарь в хранилище.
    pub fn save(&self, store: &dyn SignStore) -> Result<(), LibraryError> {
        store.save(&self.signs)
    }

    fn index_by_word(&self, word: &str) -> Option<usize> {
        let needle = word.to_lowercase();
        self.signs
            .iter()
            .position(|s| s.word.to_lowercase() == needle)
    }

    /// Диагностика: сколько примеров у каждого жеста.
    pub fn stats(&self) -> Vec<(String, usize, usize)> {
        self.signs
            .iter()
            .map(|s| (s.word.clone(), s.samples.len(), s.sequences.len()))
            .collect()
    }
}

/// Ошибки работы со словарём.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("не удалось прочитать файл {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("файл словаря повреждён: {0}")]
    Format(String),
    /// Хранилище занято: другой поток упал, удерживая блокировку.
    #[error("хранилище жестов заблокировано другим потоком")]
    Lock,
}
