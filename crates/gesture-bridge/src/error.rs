//! Ошибки моста.

use gesture_core::LibraryError;

/// Что может не получиться на границе «ядро → интерфейс».
///
/// Отдельные детали кадра сюда не попадают: плохая кисть не ломает
/// разбор целиком, а попадает в отчёт о кадре — иначе один битый кадр
/// молча остановил бы поток.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// Словарь не удалось прочитать или записать.
    #[error("не удалось сохранить жесты: {0}")]
    Library(#[from] LibraryError),

    /// Операция требует настройки, которой у моста нет.
    #[error("операция недоступна: {0}")]
    Unavailable(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_names_the_missing_setup() {
        let error = BridgeError::Unavailable("путь к словарю не задан");
        assert!(error.to_string().contains("путь"), "{error}");
    }
}
