//! Жест пользователя: слово и набранные примеры.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::HandFeatures;
use crate::motion::MotionFeatures;

/// Жест пользователя: слово + набранные примеры.
///
/// * `samples` — векторы признаков позы для статичных жестов.
/// * `sequences` — последовательности кадров для динамических.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSign {
    /// Идентификатор для обращения из интерфейса.
    /// В старых файлах может отсутствовать — тогда создаётся новый.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Слово, которому соответствует жест.
    pub word: String,
    /// Примеры статичной позы: по одному вектору на кадр.
    #[serde(default)]
    pub samples: Vec<Vec<f32>>,
    /// Примеры динамики: последовательности кадров.
    #[serde(default)]
    pub sequences: Vec<Vec<Vec<f32>>>,
}

impl CustomSign {
    pub fn new(word: &str) -> Self {
        CustomSign {
            id: Uuid::new_v4(),
            word: word.to_string(),
            samples: Vec::new(),
            sequences: Vec::new(),
        }
    }

    pub fn title(&self) -> String {
        format!("Свой жест «{}»", self.word)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty() && self.sequences.is_empty()
    }

    /// Сколько рук использовано в примерах: 1 или 2.
    pub fn hand_count(&self) -> usize {
        if let Some(first) = self.samples.first() {
            return HandFeatures::hand_count(first.len());
        }
        if let Some(frame) = self.sequences.iter().flatten().next() {
            return HandFeatures::hand_count(frame.len());
        }
        1
    }

    /// Шаблоны динамики, приведённые к виду, удобному для DTW.
    pub fn prepared_templates(&self) -> Vec<Vec<Vec<f32>>> {
        self.sequences
            .iter()
            .map(|seq| MotionFeatures::prepare_template(seq))
            .collect()
    }
}
