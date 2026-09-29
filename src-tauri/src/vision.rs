//! Поток детектора кистей: кадр камеры → 21 точка → мост.
//!
//! Детектор читает последний кадр превью, находит кисти и отдаёт точки в
//! тот же мост, что и команда `push_frame`. Благодаря этому распознавание,
//! запись жестов и словарь работают с настоящими руками, а не только с
//! демонстрационными позами интерфейса.

use std::time::{Duration, Instant};

use gesture_bridge::FrameInput;
use gesture_core::pipeline::PointSer;
use gesture_vision::OnnxHandDetector;
use tauri::{AppHandle, Manager};

use crate::camera::CameraState;
use crate::config::DetectorSettings;
use crate::state::{publish, AppStateInner};

/// Сколько раз в секунду прогонять детектор.
///
/// Выше смысла нет: камера отдаёт тридцать кадров, а распознаванию хватает
/// пятнадцати, зато процессор не занят целиком.
const TARGET_FPS: u64 = 15;

/// Запускает поток детектора. При отключённом детекторе поток не создаётся.
pub fn spawn(app: AppHandle) {
    let settings = DetectorSettings::load();
    if !settings.enabled {
        tracing::info!("детектор кистей выключен настройкой");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("gesture-detector".into())
        .spawn(move || run(app, settings));
    if let Err(error) = spawned {
        tracing::error!("не удалось запустить поток детектора: {error}");
    }
}

/// Загружает модели и крутит детекцию, пока живёт окно.
fn run(app: AppHandle, settings: DetectorSettings) {
    let mut detector = match OnnxHandDetector::load(settings.detector_config()) {
        Ok(detector) => detector,
        Err(error) => {
            tracing::error!("детектор кистей не запущен: {error}");
            return;
        }
    };
    tracing::info!(
        "детектор кистей: {} и {}",
        settings.palm_model.display(),
        settings.landmark_model.display()
    );

    let interval = Duration::from_millis(1000 / TARGET_FPS.max(1));
    loop {
        let started = Instant::now();
        process_once(&app, &mut detector);
        let elapsed = started.elapsed();
        if elapsed < interval {
            std::thread::sleep(interval - elapsed);
        }
    }
}

/// Один прогон: взять свежий кадр, найти кисти, отдать их мосту.
fn process_once(app: &AppHandle, detector: &mut OnnxHandDetector) {
    let Some(frame) = app.state::<CameraState>().frame() else {
        // Камера выключена: ждём её появления, не крутя цикл вхолостую.
        std::thread::sleep(Duration::from_millis(50));
        return;
    };

    let hands = match detector.detect(&frame) {
        Ok(hands) => hands,
        Err(error) => {
            tracing::warn!("детектор кистей: {error}");
            std::thread::sleep(Duration::from_millis(200));
            return;
        }
    };

    let hands: Vec<Vec<PointSer>> = hands
        .iter()
        .map(|hand| {
            hand.points
                .iter()
                .map(|point| PointSer {
                    x: point.x,
                    y: point.y,
                })
                .collect()
        })
        .collect();

    // Время берём у моста: кадры и команды должны идти по одной шкале, иначе
    // запись жеста снова начнёт отбрасывать кадры как устаревшие.
    let state = app.state::<AppStateInner>();
    let result = state.with_bridge(|bridge| {
        let time = bridge.now();
        bridge.ingest(&FrameInput { time, hands })
    });
    publish(app, &result);
}
