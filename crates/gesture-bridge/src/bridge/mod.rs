//! Мост между ядром и интерфейсом.
//!
//! Ядро (`gesture-core`) ничего не знает про окно, JSON и потоки, поэтому
//! повторить его логику здесь нельзя. Мост не переигрывает решение ядра,
//! а только переводит его на язык интерфейса: принимает кадр, отдаёт готовое
//! состояние и события.
//!
//! ```text
//!   камера → FrameInput → GestureBridge → AppState + EngineEvent → UI
//!                        │
//!                        └─ GestureEngine (gesture-core)
//! ```
//!
//! Благодаря такой прослойке вся логика приложения проверяется обычным
//! `cargo test`, а окно Tauri получает тонкий слой команд без тестов.

mod commands;
mod ingest;
mod library;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gesture_core::pipeline::EngineEvent;
use gesture_core::{GestureEngine, JsonFileStore, SilentSpeaker};

use crate::clock::{MonotonicClock, Seconds, SharedClock};
use crate::state::AppState;

/// Кисть, отброшенная из-за неверного числа точек.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RejectedHand {
    /// Номер кисти в кадре, начиная с нуля.
    pub hand: usize,
    /// Сколько точек пришло.
    pub received: usize,
    /// Сколько точек ожидает ядро.
    pub expected: usize,
}

/// Что показать интерфейсу после одной операции.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BridgeResult {
    /// Состояние для отрисовки.
    pub state: AppState,
    /// События: речь, команды, уведомления.
    pub event: EngineEvent,
    /// Кисти, отброшенные из-за неверного числа точек.
    pub rejected: Vec<RejectedHand>,
    /// Кадр пришёл с устаревшей меткой и не разобран.
    pub stale: bool,
}

impl BridgeResult {
    /// Ответ без событий и отбракованных кистей.
    pub(crate) fn quiet(state: AppState) -> Self {
        Self {
            state,
            event: EngineEvent::default(),
            rejected: Vec::new(),
            stale: false,
        }
    }
}

/// Счётчики потока кадров.
///
/// Нужен, чтобы отличить «жест не распознан» от «камера не поставляет
/// кадры» или «слой зрения отдаёт битые точки».
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BridgeMetrics {
    /// Сколько кадров дошло до ядра.
    pub frames_ingested: u64,
    /// Сколько кадров отброшено из-за неверной метки времени.
    pub frames_stale: u64,
    /// Сколько кистей отброшено из-за неверного числа точек.
    pub hands_rejected: u64,
}

/// Мост: один источник правды для интерфейса.
#[derive(Debug)]
pub struct GestureBridge {
    pub(crate) engine: GestureEngine,
    pub(crate) clock: SharedClock,
    /// Путь к словарю, если он должен переживать перезапуск.
    pub(crate) store_path: Option<PathBuf>,
    /// Метка последнего принятого кадра.
    pub(crate) last_time: Seconds,
    metrics: BridgeMetrics,
}

impl GestureBridge {
    /// Мост в памяти: словарь живёт до конца сеанса.
    pub fn new() -> Self {
        Self::with_clock(Arc::new(MonotonicClock::new()))
    }

    /// Мост с заданным источником времени.
    pub fn with_clock(clock: SharedClock) -> Self {
        Self::build(GestureEngine::new(), clock, None)
    }

    /// Мост со словарём в JSON-файле.
    ///
    /// Отсутствующий файл считается пустым словарём, а не ошибкой: первый
    /// запуск приложения обязан работать.
    pub fn with_file(path: impl Into<PathBuf>) -> Self {
        Self::with_file_and_clock(path, Arc::new(MonotonicClock::new()))
    }

    /// Мост со словарём в файле и заданным источником времени.
    pub fn with_file_and_clock(path: impl Into<PathBuf>, clock: SharedClock) -> Self {
        let path = path.into();
        let store = JsonFileStore::new(path.clone());
        let engine = GestureEngine::load(Box::new(store), Box::new(SilentSpeaker));
        Self::build(engine, clock, Some(path))
    }

    fn build(engine: GestureEngine, clock: SharedClock, store_path: Option<PathBuf>) -> Self {
        Self {
            engine,
            clock,
            store_path,
            last_time: f32::NEG_INFINITY,
            metrics: BridgeMetrics::default(),
        }
    }

    /// Снимок состояния для отрисовки без разбора кадра.
    pub fn state(&self) -> AppState {
        self.wrap(self.engine.snapshot(&[]))
    }

    /// Путь к файлу словаря, если он задан.
    pub fn store_path(&self) -> Option<&Path> {
        self.store_path.as_deref()
    }

    /// Счётчики потока кадров.
    pub fn metrics(&self) -> BridgeMetrics {
        self.metrics
    }

    /// Текущее время по часам моста.
    pub fn now(&self) -> Seconds {
        self.clock.now()
    }

    pub(crate) fn wrap(&self, engine: gesture_core::EngineSnapshot) -> AppState {
        AppState::new(engine, self.signs())
    }
}

impl Default for GestureBridge {
    fn default() -> Self {
        Self::new()
    }
}
