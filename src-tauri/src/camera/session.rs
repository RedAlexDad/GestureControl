//! Сессия захвата: фоновый поток и общее с ним состояние.
//!
//! Поток кадров и команды не делят мьютекс: превью лежит в отдельном
//! `Arc`, иначе чтение кадра задерживало бы ответы интерфейса.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use gesture_vision::{FfmpegCamera, FrameSource, RgbFrame};

use super::measurement::Measurement;
use super::preview_size;

/// Последний уменьшенный кадр, общий для команд и потока кадров.
#[derive(Debug, Clone)]
pub struct Preview {
    /// Кадр под мьютексом: он большой, и отдавать его наружу нельзя.
    /// Сам кадр лежит в `Arc`, чтобы команда забирала только указатель, а
    /// кодирование в base64 шло уже без блокировки.
    pub frame: Arc<Mutex<Option<Arc<RgbFrame>>>>,
}

/// Состояние захвата, разделяемое между потоком камеры и командами.
#[derive(Debug)]
pub struct Session {
    /// Поток остановлен: команды и выход из окна просят его завершиться.
    pub stop: Arc<AtomicBool>,
    /// Поток дошёл до конца сам, без команды остановки.
    pub finished: Arc<AtomicBool>,
    /// Счётчик прочитанных кадров.
    pub frames: Arc<AtomicU64>,
    /// Почему поток остановился, если остановился сам.
    pub error: Arc<Mutex<Option<String>>>,
    /// Поток захвата.
    pub worker: JoinHandle<()>,
}

/// Читает кадры, пока не попросят остановиться.
pub fn capture_loop(
    camera: &mut FfmpegCamera,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
    preview: Preview,
) {
    let mut measured = Measurement::start();
    let target = preview_size();
    while !stop.load(Ordering::Relaxed) {
        match camera.next_frame() {
            Ok(frame) => {
                let total = frames.fetch_add(1, Ordering::Relaxed) + 1;
                if measured.tick(total).is_some() {
                    tracing::info!(
                        "захват: {} кадров, {:.1} fps, {}x{} -> превью {}x{}",
                        total,
                        measured.fps(),
                        frame.width,
                        frame.height,
                        target.0,
                        target.1,
                    );
                }
                if let Ok(mut slot) = preview.frame.lock() {
                    // Меньшая копия вместо полного кадра: превью не нуждается
                    // в пикселях, а держать их в памяти между кадрами незачем.
                    // Размер совпал с превью — кадр уже пригоден, копия лишняя.
                    *slot = Some(Arc::new(if (frame.width, frame.height) == target {
                        frame
                    } else {
                        frame.resized(target.0, target.1)
                    }));
                }
            }
            Err(reason) => {
                // Ошибка чтения — это конец сессии, а не повод молчать:
                // интерфейс должен показать, почему камера остановилась.
                if let Ok(mut slot) = error.lock() {
                    *slot = Some(reason.to_string());
                }
                tracing::warn!("захват камеры остановлен: {reason}");
                break;
            }
        }
    }
    finished.store(true, Ordering::Relaxed);
}
