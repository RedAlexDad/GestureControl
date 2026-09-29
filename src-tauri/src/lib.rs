//! Окно приложения: сборка Tauri и регистрация команд.
//!
//! Слой намеренно тонкий. Всё, что касается жестов, живёт в
//! `gesture-bridge` и `gesture-core`; здесь только перевод аргументов,
//! мьютекс и рассылка событий интерфейсу.
//!
//! Кадры приходят командой `push_frame`. Пока не выбран слой зрения на
//! Rust, источником точек выступает сам интерфейс; будущий детектор
//! подключится к той же команде и не затронет остальной код.
//!
//! Камера живёт отдельно: она только копит кадры для превью, потому что
//! модель распознавания ещё не выбрана и превращать пиксели в точки пока
//! некому.

pub mod camera;
pub mod commands;
pub mod config;
pub mod state;
mod vision;

use gesture_bridge::GestureBridge;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use crate::camera::CameraState;
use crate::config::{Settings, WindowSettings};
use crate::state::AppStateInner;

/// Имя файла пользовательского словаря в каталоге данных приложения.
const LIBRARY_FILE: &str = "signs.json";

/// Метка главного окна: к ней же привязаны события и разыскивание окна.
const MAIN_WINDOW: &str = "main";

/// Заголовок окна.
const WINDOW_TITLE: &str = "Управление жестами";

/// Собирает и запускает окно приложения.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Директивы EnvFilter: сначала уровень для своего кода, потом глобальный
    // для зависимостей. Прежняя строка "gesture_control_lib=info,warn" читалась
    // как цель с именем warn, а не как глобальный уровень, и глушила вывод.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,gesture_control_lib=info".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let path = library_path(app.handle())?;
            tracing::info!("словарь жестов: {}", path.display());
            // Настройки печатаем один раз при старте: по журналу видно, какие
            // значения приложение прочитало из `.env`.
            let settings = Settings::global();
            tracing::info!("настройки: {}", settings.describe());

            let state = AppStateInner::new(GestureBridge::with_file(path));
            if state.was_poisoned() {
                tracing::error!("мост пережил панику: состояние может быть неполным");
            }
            app.manage(state);
            // Камера выключена при старте: устройство занимается только
            // после явной команды, чтобы окно не держало его открытым.
            app.manage(CameraState::new());
            // Детектор кистей читает кадры превью и кормит ими мост: без
            // него распознавание работает только на демонстрационных позах.
            vision::spawn(app.handle().clone());
            // Окно создаём кодом, а не описываем в `tauri.conf.json`: размер
            // приходит из `.env` и переменных окружения, а файл конфигурации
            // читается при сборке и переменные не видит.
            build_window(app.handle(), settings.window)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_mode,
            commands::set_recognition,
            commands::set_speech,
            commands::stop_speech,
            commands::tap,
            commands::finish_phrase,
            commands::start_recording,
            commands::cancel_recording,
            commands::delete_sign,
            commands::clear_signs,
            commands::push_frame,
            commands::get_metrics,
            commands::get_settings,
            commands::now,
            commands::log_client_error,
            camera::start_camera,
            camera::stop_camera,
            camera::camera_status,
            camera::camera_frame,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить окно приложения");
}

/// Создаёт главное окно по настройкам из `.env`.
///
/// Размер и минимум приходят логическими пикселями: на экранах с
/// масштабированием физический кадр будет кратнее, но интерфейс получит
/// столько места, сколько просил.
fn build_window(
    app: &tauri::AppHandle,
    window: WindowSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let width = f64::from(window.width);
    let height = f64::from(window.height);
    WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::default())
        .title(WINDOW_TITLE)
        .inner_size(width, height)
        .min_inner_size(f64::from(window.min_width), f64::from(window.min_height))
        .resizable(true)
        .center()
        .build()?;
    Ok(())
}

/// Полный путь к файлу словаря, создавая каталог данных при нужде.
///
/// Без каталога приложение стартовало бы с пустым словарём каждый раз,
/// поэтому ошибку записи честно поднимаем наружу.
fn library_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(LIBRARY_FILE))
}
