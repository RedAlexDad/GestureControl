//! Ошибки ядра распознавания.

use std::io;

/// Ошибка входных данных кадра.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// Детектор вернул неожиданное число точек кисти.
    #[error("ожидалось {expected} точек кисти, получено {actual}")]
    HandShape { expected: usize, actual: usize },

    /// Точки в кадре не найдены: рука вне области детекции.
    #[error("точки кисти не найдены")]
    NoPoints,

    /// Детектор вернул координаты вне границ кадра.
    #[error("координаты вне кадра: x={x}, y={y}, размер {width}x{height}")]
    OutOfBounds {
        x: f32,
        y: f32,
        width: u32,
        height: u32,
    },

    /// Некорректный размер кадра.
    #[error("пустой кадр {width}x{height}")]
    EmptyFrame { width: u32, height: u32 },

    /// Детектор не загрузился или его вывод не разобран.
    #[error("детектор недоступен: {0}")]
    Model(String),
}

/// Ошибка конвейера кадров.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// Не удалось получить кадр с камеры.
    #[error("камера недоступна: {0}")]
    Camera(String),

    /// Не удалось декодировать кадр.
    #[error("не удалось декодировать кадр: {0}")]
    Decode(String),

    /// Нет ни одной камеры.
    #[error("камеры не найдены")]
    NoCamera,
}

impl From<io::Error> for PipelineError {
    fn from(e: io::Error) -> Self {
        PipelineError::Camera(e.to_string())
    }
}
