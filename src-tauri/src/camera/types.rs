//! Типы камеры, которые ходят между Rust и интерфейсом.
//!
//! Имена полей совпадают на обеих сторонах: мост сериализует как есть, без
//! `rename_all`, поэтому один согласованный вид на весь IPC позволяет
//! сверять контракт с исходниками глазами, а не по памяти.

use gesture_vision::CameraConfig;
use serde::{Deserialize, Serialize};

use crate::config::Settings;

/// Запрос на включение камеры.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraRequest {
    /// Устройство Video4Linux2. Пусто — взять значение по умолчанию.
    pub device: String,
    /// Ширина кадра в пикселях. `0` — значение по умолчанию.
    pub width: u32,
    /// Высота кадра в пикселях. `0` — значение по умолчанию.
    pub height: u32,
    /// Частота кадров. `0` — значение по умолчанию.
    pub fps: u32,
}

impl CameraRequest {
    /// Превращает запрос в параметры захвата, подставляя настройки.
    ///
    /// Пустые поля отличают «не задано» от «нуля», поэтому подстановка
    /// идёт по полям, а не одним сравнением всего запроса. Значения по
    /// умолчанию берутся из общего конфига, а не из констант слоя зрения:
    /// так частота кадров и устройство меняются в одном месте.
    pub fn config(&self) -> CameraConfig {
        let fallback = Settings::global().camera_config();
        CameraConfig {
            device: if self.device.trim().is_empty() {
                fallback.device
            } else {
                self.device.trim().into()
            },
            size: (
                if self.width == 0 {
                    fallback.size.0
                } else {
                    self.width
                },
                if self.height == 0 {
                    fallback.size.1
                } else {
                    self.height
                },
            ),
            fps: if self.fps == 0 {
                fallback.fps
            } else {
                self.fps
            },
        }
    }
}

/// Состояние камеры для интерфейса.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraStatus {
    /// Идёт ли захват прямо сейчас.
    pub running: bool,
    /// Устройство, из которого идёт или шёл захват.
    pub device: String,
    /// Сколько кадров прочитано за время работы камеры.
    pub frames: u64,
    /// Почему камера остановилась, если остановилась сама.
    pub error: Option<String>,
}

/// Кадр для интерфейса: уменьшенная копия в base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraFrame {
    /// Ширина картинки в пикселях.
    pub width: u32,
    /// Высота картинки в пикселях.
    pub height: u32,
    /// Пиксели RGB24 в base64, три байта на точку.
    pub rgb: String,
}

/// Статус и кадр одним ответом: интерфейс опрашивает и то и сразу.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraFrameResult {
    /// Состояние камеры на момент кадра.
    pub status: CameraStatus,
    /// Кадр превью, если камера уже успела его прочитать.
    pub frame: Option<CameraFrame>,
}
