//! Команды Tauri для управления камерой и чтения кадра.

use tauri::Emitter;

use super::types::{CameraFrameResult, CameraRequest, CameraStatus};
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

/// Статус и последний кадр одним ответом.
///
/// Интерфейс опрашивает только эту команду: отдельные вызовы разъезжались
/// бы по времени и показывали бы кадр от прошлой сессии.
#[tauri::command]
pub fn camera_frame(camera: tauri::State<'_, CameraState>) -> CameraFrameResult {
    CameraFrameResult {
        status: camera.status(),
        frame: camera.preview(),
    }
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
