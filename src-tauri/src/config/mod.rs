//! Единые настройки приложения.
//!
//! Все параметры камеры и превью живут здесь и только здесь, чтобы значение
//! не приходилось искать по коду: раньше частота кадров была задана в
//! `gesture-vision`, а превью — константой в слое Tauri, и эти места
//! расходились.
//!
//! Значение приходит из трёх слоёв, и каждый следующий перекрывает
//! предыдущий: встроенное значение по умолчанию, файл `.env` и переменная
//! окружения процесса. Переменная окружения — самый верхний слой, поэтому
//! запуск с `GESTURE_CAMERA_FPS=15` перекрывает и `.env`, и умолчание.
//!
//! Файл ищется от текущего каталога вверх: `make dev` запускает приложение
//! из `src-tauri`, поэтому искать только в корне нельзя. Точный путь задаёт
//! переменная `GESTURE_ENV_FILE`.
//!
//! Разбор `.env` написан вручную: ради шести чисел тянуть отдельный crate
//! невыгодно. Формат намеренно простой — `КЛЮЧ=ЗНАЧЕНИЕ`, пустые строки и
//! строки с `#` пропускаются, значение можно взять в кавычки. Значение
//! после `#` не обрезается: в пути к устройству камеры `#` допустим.

mod env;
mod settings;
#[cfg(test)]
mod tests;

pub use env::EnvFile;
pub use settings::{CameraSettings, DetectorSettings, PreviewSettings, Settings, WindowSettings};

/// Имя файла с настройками по умолчанию.
const ENV_FILE: &str = ".env";

/// Переменная окружения с точным путём к файлу настроек.
const ENV_PATH_KEY: &str = "GESTURE_ENV_FILE";

/// Сколько каталогов вверх проверять в поисках `.env`.
///
/// Четыре уровня хватает для `src-tauri` внутри репозитория, а глубокий
/// поиск только замедлил бы запуск.
const ENV_SEARCH_DEPTH: usize = 4;

/// Ключи настроек камеры в `.env`.
mod keys {
    /// Устройство Video4Linux2.
    pub const CAMERA_DEVICE: &str = "GESTURE_CAMERA_DEVICE";
    /// Ширина кадра захвата.
    pub const CAMERA_WIDTH: &str = "GESTURE_CAMERA_WIDTH";
    /// Высота кадра захвата.
    pub const CAMERA_HEIGHT: &str = "GESTURE_CAMERA_HEIGHT";
    /// Частота кадров захвата.
    pub const CAMERA_FPS: &str = "GESTURE_CAMERA_FPS";
    /// Ширина превью в интерфейсе.
    pub const PREVIEW_WIDTH: &str = "GESTURE_PREVIEW_WIDTH";
    /// Высота превью в интерфейсе.
    pub const PREVIEW_HEIGHT: &str = "GESTURE_PREVIEW_HEIGHT";
    /// Ширина окна приложения.
    pub const WINDOW_WIDTH: &str = "GESTURE_WINDOW_WIDTH";
    /// Высота окна приложения.
    pub const WINDOW_HEIGHT: &str = "GESTURE_WINDOW_HEIGHT";
    /// Минимальная ширина окна.
    pub const WINDOW_MIN_WIDTH: &str = "GESTURE_WINDOW_MIN_WIDTH";
    /// Минимальная высота окна.
    pub const WINDOW_MIN_HEIGHT: &str = "GESTURE_WINDOW_MIN_HEIGHT";
    /// Включён ли детектор кистей.
    pub const DETECTOR_ENABLED: &str = "GESTURE_DETECTOR_ENABLED";
    /// Путь к модели детектора ладоней.
    pub const PALM_MODEL: &str = "GESTURE_PALM_MODEL";
    /// Путь к модели ключевых точек.
    pub const LANDMARK_MODEL: &str = "GESTURE_LANDMARK_MODEL";
    /// Порог уверенности детектора ладоней.
    pub const DETECTOR_SCORE: &str = "GESTURE_DETECTOR_SCORE";
    /// Сколько кистей искать в кадре.
    pub const DETECTOR_MAX_HANDS: &str = "GESTURE_DETECTOR_MAX_HANDS";
}
