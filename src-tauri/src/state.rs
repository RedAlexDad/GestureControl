//! Состояние окна: мост в мьютексе плюс рассылка событий.
//!
//! Мост меняется и потоком кадров, и командами интерфейса, поэтому он
//! лежит под мьютексом. Мьютекс намеренно короткоживущий: пока он занят,
//! кадр ждёт, а не наоборот — иначе интерфейс встал бы на каждом кадре.

use std::sync::Mutex;

use gesture_bridge::{AppState, BridgeResult, GestureBridge};
use tauri::{AppHandle, Emitter};

/// Событие, которое окно шлёт интерфейсу после каждого разбора кадра.
pub const STATE_EVENT: &str = "gesture://state";

/// Состояние приложения, разделяемое между потоками.
#[derive(Debug)]
pub struct AppStateInner {
    /// Мост: единственный источник правды о жестах и словаре.
    bridge: Mutex<GestureBridge>,
    /// Ошибки доступа к мьютексу не должны ронять поток кадров.
    poisoned: Mutex<bool>,
}

impl AppStateInner {
    /// Создаёт состояние поверх готового моста.
    pub fn new(bridge: GestureBridge) -> Self {
        Self {
            bridge: Mutex::new(bridge),
            poisoned: Mutex::new(false),
        }
    }

    /// Выполняет действие над мостом, не отдавая блокировку наружу.
    ///
    /// Отравленный мьютекс переиспользуется: жесты уже в памяти, паника
    /// случилась в чужом кадре, а терять словарь из-за этого нельзя.
    pub fn with_bridge<T>(&self, action: impl FnOnce(&mut GestureBridge) -> T) -> T {
        let mut guard = match self.bridge.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                self.mark_poisoned();
                poisoned.into_inner()
            }
        };
        action(&mut guard)
    }

    /// Состояние для отрисовки.
    pub fn snapshot(&self) -> AppState {
        self.with_bridge(|bridge| bridge.state())
    }

    /// Отмечает, что мьютекс пришлось восстанавливать после паники.
    fn mark_poisoned(&self) {
        if let Ok(mut flag) = self.poisoned.lock() {
            *flag = true;
        }
    }

    /// Было ли состояние отравлено паникой в чужом потоке.
    ///
    /// Молча работать на полуразобранном состоянии опаснее, чем сказать
    /// об этом, поэтому окно спрашивает флаг при старте.
    pub fn was_poisoned(&self) -> bool {
        self.poisoned.lock().map(|flag| *flag).unwrap_or(false)
    }
}

/// Отправляет интерфейсу состояние и события разбора кадра.
pub fn publish(app: &AppHandle, result: &BridgeResult) {
    if let Err(error) = app.emit(STATE_EVENT, result) {
        tracing::warn!("не удалось отправить состояние в интерфейс: {error}");
    }
    if !result.rejected.is_empty() {
        tracing::debug!(
            hands = result.rejected.len(),
            "кадр содержал кисти с неверным числом точек"
        );
    }
    if result.stale {
        tracing::trace!("кадр с устаревшей меткой времени пропущен");
    }
}

/// Ошибка команды, пригодная для отправки в интерфейс.
///
/// Tauri требует, чтобы ошибка команды сериализовалась, а доменные
/// ошибки моста для этого не предназначены.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CommandError {
    /// Текст для показа пользователю.
    pub message: String,
}

impl CommandError {
    /// Оборачивает доменную ошибку.
    pub fn new(message: impl std::fmt::Display) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

/// Превращает отказ в ошибку команды и пишет его в журнал окна.
///
/// Без этой записи отказ жил только в подсказке интерфейса: по журналу
/// приложение выглядело исправным, хотя команда не сработала.
pub fn failure<T, E: std::fmt::Display>(command: &str, error: E) -> Result<T, CommandError> {
    tracing::warn!("команда {command} отказала: {error}");
    Err(CommandError::new(error))
}
