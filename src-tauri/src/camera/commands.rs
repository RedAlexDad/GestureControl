//! Команды Tauri для управления камерой и чтения кадра.

use tauri::Emitter;

use super::types::{CameraRequest, CameraStatus};
use super::CameraState;

/// Событие со сменой состояния камеры: включилась, остановилась, ошибка.
pub const CAMERA_EVENT: &str = "gesture://camera";

/// Команда включения камеры: открывает устройство и запускает поток.
#[tauri::command]
pub fn start_camera(
    app: tauri::AppHandle,
    camera: tauri::State<'_, CameraState>,
    request: CameraRequest,
) -> CameraStatus {
    let status = camera.start(&request);
    emit_camera(&app, &status);
    status
}

/// Команда выключения камеры: останавливает поток и освобождает устройство.
#[tauri::command]
pub fn stop_camera(app: tauri::AppHandle, camera: tauri::State<'_, CameraState>) -> CameraStatus {
    let status = camera.stop();
    emit_camera(&app, &status);
    status
}

/// Текущий статус камеры без кадра.
#[tauri::command]
pub fn camera_status(camera: tauri::State<'_, CameraState>) -> CameraStatus {
    camera.status()
}

/// Последний кадр превью двоичным ответом.
///
/// Пиксели уходят как есть, без base64 и JSON: строка весила на треть
/// больше, а окно тратило время на её разбор. Заголовок ответа — ширина и
/// высота по четыре байта, младшим байтом вперёд, дальше готовый RGBA;
/// пустой ответ означает, что кадра пока нет. Статус интерфейс спрашивает
/// отдельной командой.
#[tauri::command]
pub fn camera_frame(camera: tauri::State<'_, CameraState>) -> tauri::ipc::Response {
    tauri::ipc::Response::new(camera.packed_frame().unwrap_or_default())
}

/// Отправляет интерфейсу смену состояния камеры.
fn emit_camera(app: &tauri::AppHandle, status: &CameraStatus) {
    if let Err(error) = app.emit(CAMERA_EVENT, status) {
        tracing::warn!("не удалось отправить состояние камеры: {error}");
    }
    if let Some(reason) = &status.error {
        tracing::error!("камера недоступна: {reason}");
    }
}
