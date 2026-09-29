//! Окно приложения: сборка Tauri и регистрация команд.
//!
//! Слой намеренно тонкий. Всё, что касается жестов, живёт в
//! `gesture-bridge` и `gesture-core`; здесь только перевод аргументов,
//! мьютекс и рассылка событий интерфейсу.
//!
//! Кадры приходят командой `push_frame`. Пока не выбран слой зрения на
//! Rust, источником точек выступает сам интерфейс; будущий детектор
//! подключится к той же команде и не затронет остальной код.

pub mod commands;
pub mod state;

use gesture_bridge::GestureBridge;
use tauri::Manager;

use crate::state::AppStateInner;

/// Имя файла пользовательского словаря в каталоге данных приложения.
const LIBRARY_FILE: &str = "signs.json";

/// Собирает и запускает окно приложения.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gesture_control_lib=info,warn".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let path = library_path(app.handle())?;
            tracing::info!("словарь жестов: {}", path.display());

            let state = AppStateInner::new(GestureBridge::with_file(path));
            if state.was_poisoned() {
                tracing::error!("мост пережил панику: состояние может быть неполным");
            }
            app.manage(state);
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
            commands::now,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить окно приложения");
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
