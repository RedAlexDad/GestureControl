//! Поток детектора кистей: кадр камеры → 21 точка → мост.
//!
//! Основной детектор — официальный MediaPipe Hand Landmarker: он сам находит
//! кисть и ведёт её между кадрами, поэтому скелет ровнее, а точки точнее.
//! Если модель или библиотека не найдены, берётся запасной ONNX-детектор.

use std::time::{Duration, Instant};

use gesture_bridge::FrameInput;
use gesture_core::pipeline::PointSer;
use gesture_core::VisionError;
use gesture_vision::{HandLandmarks, OnnxHandDetector, RgbFrame};
use tauri::{AppHandle, Manager};

use crate::camera::CameraState;
use crate::config::DetectorSettings;
use crate::state::{publish, AppStateInner};

/// Сколько раз в секунду прогонять детектор.
///
/// MediaPipe ведёт отслеживание по времени между кадрами, поэтому частота
/// совпадает с частотой камеры.
const TARGET_FPS: u64 = 30;

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

/// Детектор кистей: официальный MediaPipe, а при его отсутствии — ONNX.
enum Backend {
    MediaPipe(Box<gesture_vision::detector::mediapipe::MediaPipeHandDetector>),
    Onnx(Box<OnnxHandDetector>),
}

impl Backend {
    /// Загружает лучший доступный детектор.
    fn load(settings: &DetectorSettings) -> Result<Self, VisionError> {
        let mediapipe = settings.mediapipe_config();
        if mediapipe.library.is_file() && mediapipe.model.is_file() {
            match gesture_vision::detector::mediapipe::MediaPipeHandDetector::load(&mediapipe) {
                Ok(detector) => {
                    tracing::info!(
                        "детектор кистей: MediaPipe {} и {}",
                        mediapipe.library.display(),
                        mediapipe.model.display()
                    );
                    return Ok(Backend::MediaPipe(Box::new(detector)));
                }
                Err(error) => {
                    tracing::error!("MediaPipe не запустился: {error}; беру ONNX");
                }
            }
        } else {
            tracing::info!("моделей MediaPipe нет, беру ONNX");
        }
        let onnx = OnnxHandDetector::load(settings.detector_config())?;
        Ok(Backend::Onnx(Box::new(onnx)))
    }

    /// Ищет кисти в кадре.
    fn detect(&mut self, frame: &RgbFrame) -> Result<Vec<HandLandmarks>, VisionError> {
        match self {
            Backend::MediaPipe(detector) => detector.detect(frame),
            Backend::Onnx(detector) => detector.detect(frame),
        }
    }
}

/// Загружает модели и крутит детекцию, пока живёт окно.
fn run(app: AppHandle, settings: DetectorSettings) {
    let mut detector = match Backend::load(&settings) {
        Ok(detector) => detector,
        Err(error) => {
            tracing::error!("детектор кистей не запущен: {error}");
            return;
        }
    };

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
fn process_once(app: &AppHandle, detector: &mut Backend) {
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
