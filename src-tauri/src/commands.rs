//! Команды, которые окно предоставляет интерфейсу.
//!
//! Каждая команда тонкая: она переводит аргументы, зовёт мост и
//! отдаёт готовое состояние. Разбор кадров, распознавание и словарь
//! живут в `gesture-core`; здесь нет ни одной строки логики жестов.

use gesture_bridge::{AppMode, AppState, BridgeResult, FrameInput};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::state::{publish, AppStateInner, CommandError};

/// Состояние приложения, которым владеет Tauri.
pub type Shared<'a> = State<'a, AppStateInner>;

/// Текущее состояние: режим, фраза, запись, скины и словарь.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetState {
    /// Снимок для отрисовки.
    pub state: AppState,
    /// Счётчики потока кадров: принято, отброшено по времени, отброшено
    /// по числу точек.
    pub metrics: Metrics,
}

/// Счётчики потока кадров в том же виде, что отдаёт мост.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    /// Сколько кадров дошло до ядра.
    pub frames_ingested: u64,
    /// Сколько кадров отброшено из-за устаревшей метки времени.
    pub frames_stale: u64,
    /// Сколько кистей отброшено из-за неверного числа точек.
    pub hands_rejected: u64,
}

impl From<gesture_bridge::BridgeMetrics> for Metrics {
    fn from(value: gesture_bridge::BridgeMetrics) -> Self {
        Self {
            frames_ingested: value.frames_ingested,
            frames_stale: value.frames_stale,
            hands_rejected: value.hands_rejected,
        }
    }
}

/// Запрос на смену режима: `Control` или `Translate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMode {
    /// Имя режима: `Control` или `Translate`.
    pub mode: AppMode,
}

/// Запрос на включение или выключение распознавания.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFlag {
    /// Новое значение флага.
    pub enabled: bool,
}

/// Запрос на запись нового жеста.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecording {
    /// Слово, которому соответствует жест.
    pub word: String,
    /// `true` — жест по движению, `false` — по позе.
    pub is_dynamic: bool,
}

/// Запрос на удаление жеста из словаря.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSign {
    /// Идентификатор жеста из списка.
    pub id: String,
}

/// Запрос на переключение экрана демо-интерфейса.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchScreen {
    /// Номер экрана, на который нужно перейти.
    pub index: usize,
}

/// Отдаёт интерфейсу полное состояние и счётчики.
#[tauri::command]
pub fn get_state(app: Shared<'_>) -> GetState {
    let (state, metrics) = app.with_bridge(|bridge| (bridge.state(), bridge.metrics()));
    GetState {
        state,
        metrics: metrics.into(),
    }
}

/// Переключает режим: управление или сурдоперевод.
#[tauri::command]
pub fn set_mode(app: Shared<'_>, request: SetMode) -> AppState {
    app.with_bridge(|bridge| bridge.set_mode(request.mode))
}

/// Включает или выключает распознавание.
#[tauri::command]
pub fn set_recognition(app: Shared<'_>, request: SetFlag) -> AppState {
    app.with_bridge(|bridge| bridge.set_recognition(request.enabled))
}

/// Включает или выключает озвучивание.
#[tauri::command]
pub fn set_speech(app: Shared<'_>, request: SetFlag) -> AppState {
    app.with_bridge(|bridge| bridge.set_speech(request.enabled))
}

/// Останавливает текущую речь.
#[tauri::command]
pub fn stop_speech(app: Shared<'_>) -> AppState {
    app.with_bridge(|bridge| bridge.stop_speech())
}

/// Касание по экрану: работает как жест `Tap`.
#[tauri::command]
pub fn tap(app: Shared<'_>, request: SwitchScreen) -> BridgeResult {
    app.with_bridge(|bridge| bridge.tap(request.index))
}

/// Завершает накопленную фразу: произносит её и очищает.
#[tauri::command]
pub fn finish_phrase(app: Shared<'_>) -> BridgeResult {
    app.with_bridge(|bridge| bridge.finish_phrase())
}

/// Начинает запись нового жеста: сначала обратный отсчёт.
#[tauri::command]
pub fn start_recording(app: Shared<'_>, request: StartRecording) -> Result<AppState, CommandError> {
    let word = request.word.trim();
    if word.is_empty() {
        return Err(CommandError::new("для записи нужно слово"));
    }
    Ok(app.with_bridge(|bridge| bridge.start_recording(word, request.is_dynamic)))
}

/// Отменяет запись, если она идёт.
#[tauri::command]
pub fn cancel_recording(app: Shared<'_>) -> AppState {
    app.with_bridge(|bridge| bridge.cancel_recording())
}

/// Удаляет жест из словаря по его идентификатору.
#[tauri::command]
pub fn delete_sign(app: Shared<'_>, request: DeleteSign) -> AppState {
    app.with_bridge(|bridge| bridge.delete_sign(&request.id))
}

/// Очищает словарь целиком и сохраняет пустоту на диск.
#[tauri::command]
pub fn clear_signs(app: Shared<'_>) -> Result<AppState, CommandError> {
    app.with_bridge(|bridge| bridge.clear_signs())
        .map_err(CommandError::new)
}

/// Принимает кадр с 21 точкой на кисть.
///
/// Это единственный путь для кадров: пока нет выбранного слоя зрения,
/// источником кадров выступает сам интерфейс, а будущий детектор
/// подключится здесь же, не меняя остальной код.
#[tauri::command]
pub fn push_frame(app: AppHandle, state: Shared<'_>, frame: FrameInput) -> BridgeResult {
    let result = state.with_bridge(|bridge| bridge.ingest(&frame));
    publish(&app, &result);
    result
}

/// Счётчики потока кадров без полного состояния.
#[tauri::command]
pub fn get_metrics(app: Shared<'_>) -> Metrics {
    app.with_bridge(|bridge| bridge.metrics()).into()
}

/// Текущее время моста в секундах.
#[tauri::command]
pub fn now(app: Shared<'_>) -> f32 {
    app.with_bridge(|bridge| bridge.now())
}
