//! Структуры настроек и их сборка из слоёв окружения.

use std::path::PathBuf;
use std::sync::OnceLock;

use gesture_vision::CameraConfig;
use serde::{Deserialize, Serialize};

use super::env::EnvFile;
use super::keys;

/// Настройки захвата камеры.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraSettings {
    /// Устройство Video4Linux2.
    pub device: String,
    /// Ширина кадра захвата в пикселях.
    pub width: u32,
    /// Высота кадра захвата в пикселях.
    pub height: u32,
    /// Частота кадров захвата.
    pub fps: u32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        CameraSettings {
            device: "/dev/video0".into(),
            width: 640,
            height: 480,
            // Тридцать кадров в секунду — штатный режим этой камеры.
            // Значение 15 из прошлых версий было заниженным дефолтом, а не
            // ограничением устройства.
            fps: 30,
        }
    }
}

/// Настройки уменьшенной копии кадра для интерфейса.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSettings {
    /// Ширина превью в пикселях.
    pub width: u32,
    /// Высота превью в пикселях.
    pub height: u32,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        // Превью равно размеру захвата: интерфейс рисует кадр без уменьшения,
        // поэтому картинка остаётся резкой. RGB24 на кадр — 921 600 байт, они
        // уходят в интерфейс как base64 в JSON-ответе, поэтому частота
        // опроса на стороне окна ограничена размером кадра.
        PreviewSettings {
            width: 640,
            height: 480,
        }
    }
}

/// Настройки окна приложения.
///
/// Раньше размер жил в `tauri.conf.json`, откуда его нельзя было перекрыть
/// переменной окружения: файл собирается в момент компиляции. Теперь окно
/// создаётся кодом и читает те же `.env` и переменные окружения, что и
/// камера с превью.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSettings {
    /// Ширина окна в логических пикселях.
    pub width: u32,
    /// Высота окна в логических пикселях.
    pub height: u32,
    /// Минимальная ширина, до которой окно можно сжать.
    pub min_width: u32,
    /// Минимальная высота, до которой окно можно сжать.
    pub min_height: u32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        // Значения прежнего `tauri.conf.json`: окно на демо-интерфейс
        // должно вмещать таблицу жестов и ленту кадров.
        WindowSettings {
            width: 1180,
            height: 840,
            min_width: 900,
            min_height: 640,
        }
    }
}

impl WindowSettings {
    /// Подгоняет минимальный размер под основной.
    ///
    /// Минимум больше окна — опечатка в `.env`, из-за которой окно не
    /// открылось бы или сразу схлопнулось. Молча чинить нельзя: пишем
    /// предупреждение и уменьшаем минимум до основного размера.
    fn fit_minimum(&mut self) {
        if self.min_width > self.width {
            tracing::warn!(
                "{}: {} больше ширины окна {}, беру ширину окна",
                keys::WINDOW_MIN_WIDTH,
                self.min_width,
                self.width
            );
            self.min_width = self.width;
        }
        if self.min_height > self.height {
            tracing::warn!(
                "{}: {} больше высоты окна {}, беру высоту окна",
                keys::WINDOW_MIN_HEIGHT,
                self.min_height,
                self.height
            );
            self.min_height = self.height;
        }
    }
}

/// Настройки детектора кистей.
///
/// Детектор живёт в отдельном потоке и читает настройки сам: держать их в
/// общем [`Settings`] незачем, а окну они не нужны.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectorSettings {
    /// Включён ли детектор. Выключенный не грузит модели вовсе.
    pub enabled: bool,
    /// Путь к модели детектора ладоней.
    pub palm_model: PathBuf,
    /// Путь к модели ключевых точек.
    pub landmark_model: PathBuf,
    /// Путь к `libmediapipe.so` официального MediaPipe.
    pub mediapipe_library: PathBuf,
    /// Путь к модели `hand_landmarker.task`.
    pub mediapipe_model: PathBuf,
    /// Порог уверенности детектора ладоней.
    pub score_threshold: f32,
    /// Сколько кистей искать в кадре.
    pub max_hands: usize,
}

impl Default for DetectorSettings {
    fn default() -> Self {
        DetectorSettings {
            enabled: true,
            palm_model: PathBuf::from("models/palm_detection.onnx"),
            landmark_model: PathBuf::from("models/hand_landmark.onnx"),
            mediapipe_library: PathBuf::from("models/libmediapipe.so"),
            mediapipe_model: PathBuf::from("models/hand_landmarker.task"),
            score_threshold: 0.5,
            max_hands: 2,
        }
    }
}

impl DetectorSettings {
    /// Читает настройки из окружения и файла `.env`.
    pub fn load() -> DetectorSettings {
        let file = EnvFile::discover();
        DetectorSettings::from_source(|key| {
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| file.get(key).map(str::to_string))
        })
    }

    /// Собирает настройки из произвольного источника по ключу.
    pub fn from_source<F>(lookup: F) -> DetectorSettings
    where
        F: Fn(&str) -> Option<String>,
    {
        let defaults = DetectorSettings::default();
        // Модели лежат в каталоге `models` в корне репозитория, а запуск
        // идёт из `src-tauri`, поэтому ищем каталог вверх по дереву.
        let models = discover_models_dir();
        let palm = models
            .as_ref()
            .map(|dir| dir.join("palm_detection.onnx"))
            .unwrap_or(defaults.palm_model);
        let landmark = models
            .as_ref()
            .map(|dir| dir.join("hand_landmark.onnx"))
            .unwrap_or(defaults.landmark_model);
        let mediapipe_library = models
            .as_ref()
            .map(|dir| dir.join("libmediapipe.so"))
            .unwrap_or(defaults.mediapipe_library);
        let mediapipe_model = models
            .as_ref()
            .map(|dir| dir.join("hand_landmarker.task"))
            .unwrap_or(defaults.mediapipe_model);

        DetectorSettings {
            enabled: read_flag(&lookup, keys::DETECTOR_ENABLED, true),
            palm_model: PathBuf::from(read_text(
                &lookup,
                keys::PALM_MODEL,
                &palm.to_string_lossy(),
            )),
            landmark_model: PathBuf::from(read_text(
                &lookup,
                keys::LANDMARK_MODEL,
                &landmark.to_string_lossy(),
            )),
            mediapipe_library: PathBuf::from(read_text(
                &lookup,
                keys::MEDIAPIPE_LIB,
                &mediapipe_library.to_string_lossy(),
            )),
            mediapipe_model: PathBuf::from(read_text(
                &lookup,
                keys::HAND_TASK,
                &mediapipe_model.to_string_lossy(),
            )),
            score_threshold: read_float(&lookup, keys::DETECTOR_SCORE, 0.5),
            max_hands: read_number(&lookup, keys::DETECTOR_MAX_HANDS, 2) as usize,
        }
    }

    /// Настройки официального MediaPipe Hand Landmarker.
    pub fn mediapipe_config(&self) -> gesture_vision::detector::mediapipe::MediaPipeConfig {
        gesture_vision::detector::mediapipe::MediaPipeConfig {
            library: self.mediapipe_library.clone(),
            model: self.mediapipe_model.clone(),
            max_hands: self.max_hands,
            min_detection: self.score_threshold,
            ..gesture_vision::detector::mediapipe::MediaPipeConfig::default()
        }
    }

    /// Параметры детектора для слоя зрения.
    pub fn detector_config(&self) -> gesture_vision::DetectorConfig {
        gesture_vision::DetectorConfig {
            palm_model: self.palm_model.clone(),
            landmark_model: self.landmark_model.clone(),
            score_threshold: self.score_threshold,
            max_hands: self.max_hands,
            ..gesture_vision::DetectorConfig::default()
        }
    }
}

/// Ищет каталог `models` вверх по дереву от текущего каталога.
fn discover_models_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..=super::ENV_SEARCH_DEPTH {
        let candidate = dir.join("models");
        if candidate.join("palm_detection.onnx").is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Все настройки приложения.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Захват камеры.
    pub camera: CameraSettings,
    /// Превью в интерфейсе.
    pub preview: PreviewSettings,
    /// Размер окна приложения.
    pub window: WindowSettings,
}

impl Settings {
    /// Настройки процесса: умолчание, `.env`, переменные окружения.
    ///
    /// Результат кешируется, потому что настройки не меняются за время работы
    /// окна, а читать файл из каждого команды камеры незачем.
    pub fn global() -> &'static Settings {
        static GLOBAL: OnceLock<Settings> = OnceLock::new();
        GLOBAL.get_or_init(Settings::load)
    }

    /// Читает настройки из окружения и файла `.env`.
    pub fn load() -> Settings {
        let file = EnvFile::discover();
        Settings::from_source(|key| {
            // Переменная окружения перекрывает файл. Пустое значение считаем
            // незаданным: `GESTURE_CAMERA_FPS=` в окружении не должен
            // стирать число из `.env`.
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| file.get(key).map(str::to_string))
        })
    }

    /// Собирает настройки из произвольного источника по ключу.
    ///
    /// Отдельный источник нужен тестам: переменные окружения общие у всего
    /// процесса, и проверка через них ломала бы тесты, идущие следом.
    pub fn from_source<F>(lookup: F) -> Settings
    where
        F: Fn(&str) -> Option<String>,
    {
        let camera = CameraSettings {
            device: read_text(&lookup, keys::CAMERA_DEVICE, "/dev/video0"),
            width: read_number(&lookup, keys::CAMERA_WIDTH, 640),
            height: read_number(&lookup, keys::CAMERA_HEIGHT, 480),
            fps: read_number(&lookup, keys::CAMERA_FPS, 30),
        };
        let preview = PreviewSettings {
            width: read_number(&lookup, keys::PREVIEW_WIDTH, 640),
            height: read_number(&lookup, keys::PREVIEW_HEIGHT, 480),
        };
        // Умолчания окна берём из одного места: четыре числа в двух
        // списках разъедутся при первой же правке.
        let defaults = WindowSettings::default();
        let mut window = WindowSettings {
            width: read_number(&lookup, keys::WINDOW_WIDTH, defaults.width),
            height: read_number(&lookup, keys::WINDOW_HEIGHT, defaults.height),
            min_width: read_number(&lookup, keys::WINDOW_MIN_WIDTH, defaults.min_width),
            min_height: read_number(&lookup, keys::WINDOW_MIN_HEIGHT, defaults.min_height),
        };
        window.fit_minimum();
        Settings {
            camera,
            preview,
            window,
        }
    }

    /// Параметры захвата для слоя зрения.
    pub fn camera_config(&self) -> CameraConfig {
        CameraConfig {
            device: PathBuf::from(&self.camera.device),
            size: (self.camera.width, self.camera.height),
            fps: self.camera.fps,
        }
    }

    /// Размер превью как есть отдаётся в resize.
    pub fn preview_size(&self) -> (u32, u32) {
        (self.preview.width, self.preview.height)
    }

    /// Строка для журнала запуска: что приложение решило использовать.
    pub fn describe(&self) -> String {
        format!(
            "окно {}x{} (минимум {}x{}), камера {} {}x{} @ {} fps, превью {}x{}",
            self.window.width,
            self.window.height,
            self.window.min_width,
            self.window.min_height,
            self.camera.device,
            self.camera.width,
            self.camera.height,
            self.camera.fps,
            self.preview.width,
            self.preview.height,
        )
    }
}

/// Читает строковое значение, пустое откатывая к умолчанию.
fn read_text<F>(lookup: &F, key: &str, fallback: &str) -> String
where
    F: Fn(&str) -> Option<String>,
{
    match lookup(key) {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        Some(_) => {
            tracing::warn!("{key}: значение пустое, беру {fallback}");
            fallback.to_string()
        }
        None => fallback.to_string(),
    }
}

/// Читает число, отбрасывая ноль, мусор и ошибки разбора.
///
/// Ошибочное значение не должно мешать запуску окна: вместо падения
/// приложение пишет предупреждение и берёт значение по умолчанию.
fn read_number<F>(lookup: &F, key: &str, fallback: u32) -> u32
where
    F: Fn(&str) -> Option<String>,
{
    let Some(raw) = lookup(key).map(|value| value.trim().to_string()) else {
        return fallback;
    };
    match raw.parse::<u32>() {
        Ok(0) => {
            tracing::warn!("{key}: значение должно быть больше нуля, беру {fallback}");
            fallback
        }
        Ok(value) => value,
        Err(_) => {
            tracing::warn!("{key}: «{raw}» не число, беру {fallback}");
            fallback
        }
    }
}

/// Читает булево значение: `1/true/on/да` и `0/false/off/нет`.
fn read_flag<F>(lookup: &F, key: &str, fallback: bool) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    let Some(raw) = lookup(key).map(|value| value.trim().to_lowercase()) else {
        return fallback;
    };
    match raw.as_str() {
        "1" | "true" | "on" | "yes" | "да" => true,
        "0" | "false" | "off" | "no" | "нет" => false,
        _ => {
            tracing::warn!("{key}: «{raw}» не булево, беру {fallback}");
            fallback
        }
    }
}

/// Читает число с плавающей точкой, отбрасывая мусор и неположительные.
fn read_float<F>(lookup: &F, key: &str, fallback: f32) -> f32
where
    F: Fn(&str) -> Option<String>,
{
    let Some(raw) = lookup(key).map(|value| value.trim().to_string()) else {
        return fallback;
    };
    match raw.parse::<f32>() {
        Ok(value) if value.is_finite() && value > 0.0 => value,
        _ => {
            tracing::warn!("{key}: «{raw}» не положительное число, беру {fallback}");
            fallback
        }
    }
}
