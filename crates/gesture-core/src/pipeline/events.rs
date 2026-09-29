//! События и снимок состояния движка в сериализуемом виде.

use serde::{Deserialize, Serialize};

use crate::geometry::Point;
use crate::models::{AppMode, Command, RecordingState, Sign};

/// Событие, которое движок просит выполнить оболочке.
///
/// Сериализуется, потому что это полезная нагрузка IPC: оболочка
/// пересылает её в интерфейс как есть.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EngineEvent {
    /// Текст для озвучки.
    pub speak: Option<String>,
    /// Команды интерфейса в порядке выполнения.
    pub commands: Vec<Command>,
}

impl EngineEvent {
    pub fn is_empty(&self) -> bool {
        self.speak.is_none() && self.commands.is_empty()
    }
}

/// Снимок состояния движка: всё, что нужно нарисовать в интерфейсе.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub mode: AppMode,
    /// Включено ли распознавание (кнопка «Пауза»).
    pub recognition_enabled: bool,
    /// Включена ли озвучка.
    pub speech_enabled: bool,
    /// Текущий жест: событие либо удерживаемый.
    pub sign: Sign,
    /// Слово, добавленное в фразу последним.
    pub streaming_word: Option<String>,
    /// Накопленная фраза.
    pub phrase: String,
    pub recording: RecordingStateSer,
    /// Текст последнего уведомления.
    pub notice: Option<String>,
    /// Сколько слов в словаре.
    pub signs_count: usize,
    /// Индекс текущего экрана демо-интерфейса.
    pub screen_index: usize,
    /// Масштаб демо-интерфейса: производная от `parameter`.
    pub zoom: f32,
    /// Громкость: производная от `parameter`.
    pub volume: f32,
    /// Параметр, меняемый жестами увеличения/уменьшения.
    pub parameter: f32,
    /// Сколько кистей в кадре.
    pub hands_visible: usize,
    /// Координаты точек кистей для отрисовки скелета.
    pub hands: Vec<Vec<PointSer>>,
}

/// Состояние записи в сериализуемом виде.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RecordingStateSer {
    Idle,
    Countdown(u8),
    Recording(f32),
}

impl From<RecordingState> for RecordingStateSer {
    fn from(value: RecordingState) -> Self {
        match value {
            RecordingState::Idle => RecordingStateSer::Idle,
            RecordingState::Countdown(n) => RecordingStateSer::Countdown(n),
            RecordingState::Recording(p) => RecordingStateSer::Recording(p),
        }
    }
}

/// Точка в сериализуемом виде.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointSer {
    pub x: f32,
    pub y: f32,
}

impl From<Point> for PointSer {
    fn from(p: Point) -> Self {
        PointSer { x: p.x, y: p.y }
    }
}
