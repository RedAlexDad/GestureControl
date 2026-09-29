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
