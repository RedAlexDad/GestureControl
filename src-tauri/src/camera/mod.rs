//! Живой захват камеры: фоновый поток, счётчики и последний кадр.
//!
//! Захват идёт всегда, пока камера включена, а разбор кадра — только там,
//! где есть модель. Сейчас модели нет, поэтому поток лишь копит кадры для
//! превью и честно считает их: интерфейсу нужно видеть, что камера жива,
//! даже когда распознавание ещё не подключено.
//!
//! Поток кадров и команды не делят мьютекс: превью лежит в отдельном
//! `Arc`, иначе чтение кадра задерживало бы ответы интерфейса.

mod commands;
mod measurement;
mod session;
mod types;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use commands::*;
pub use types::*;

use gesture_vision::{FfmpegCamera, RgbFrame};

use crate::config::Settings;
use session::{Preview, Session};

/// Размер превью из настроек приложения.
fn preview_size() -> (u32, u32) {
    Settings::global().preview_size()
}

/// Состояние камеры, которым владеет Tauri.
#[derive(Debug)]
pub struct CameraState {
    /// Текущая сессия захвата.
    session: Mutex<Option<Session>>,
    /// Статус для интерфейса: работает ли камера и почему остановилась.
    status: Mutex<CameraStatus>,
    /// Последний кадр превью.
    preview: Preview,
}

impl Default for CameraState {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraState {
    /// Создаёт выключенную камеру.
    pub fn new() -> Self {
        CameraState {
            session: Mutex::new(None),
            status: Mutex::new(CameraStatus::default()),
            preview: Preview {
                frame: Arc::new(Mutex::new(None)),
            },
        }
    }

    /// Включает камеру, если она ещё не включена.
    ///
    /// Повторный запуск ничего не делает: обработчик кнопки может звать
    /// команду, не спрашивая у интерфейса текущее состояние.
    pub fn start(&self, request: &CameraRequest) -> CameraStatus {
        let config = request.config();
        let device = config.device.display().to_string();
        let mut slot = self.lock(&self.session);
        if let Some(current) = slot.as_ref() {
            if current.finished.load(Ordering::Relaxed) {
                // Поток дошёл до конца сам: ждём его и освобождаем место.
                if let Some(session) = slot.take() {
                    let _ = session.worker.join();
                }
            } else {
                return self.status_of(slot.as_ref());
            }
        }
        if let Err(error) = config.validated() {
            drop(slot);
            tracing::error!("запрос камеры отклонён: {error}");
            self.set_stopped(&device, error.to_string());
            return self.status();
        }
        let preview = preview_size();
        tracing::info!(
            "захват камеры: {}, {}x{} @ {} fps -> превью {}x{}",
            config.device.display(),
            config.size.0,
            config.size.1,
            config.fps,
            preview.0,
            preview.1,
        );
        let mut camera = match FfmpegCamera::start(config) {
            Ok(camera) => camera,
            Err(error) => {
                drop(slot);
                tracing::error!("камера не запустилась: {error}");
                self.set_stopped(&device, error.to_string());
                return self.status();
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
        let error = Arc::new(Mutex::new(None));
        let preview = self.preview.clone();
        let worker = {
            let stop = Arc::clone(&stop);
            let finished = Arc::clone(&finished);
            let frames = Arc::clone(&frames);
            let error = Arc::clone(&error);
            std::thread::Builder::new()
                .name("camera".into())
                .spawn(move || {
                    session::capture_loop(&mut camera, stop, finished, frames, error, preview)
                })
        };
        let worker = match worker {
            Ok(worker) => worker,
            Err(source) => {
                drop(slot);
                self.set_stopped(
                    &device,
                    format!("не удалось создать поток камеры: {source}"),
                );
                return self.status();
            }
        };
        *slot = Some(Session {
            stop,
            finished,
            frames,
            error,
            worker,
        });
        // Превью прошлой камеры не должно висеть поверх новой.
        self.clear_preview();
        self.set_running(&device);
        self.status_of(slot.as_ref())
    }

    /// Останавливает камеру и ждёт поток.
    pub fn stop(&self) -> CameraStatus {
        let device = {
            let session = self.lock(&self.session).take();
            if let Some(session) = session {
                session.stop.store(true, Ordering::Relaxed);
                // Поток сам выходит из цикла, когда видит флаг, а `Drop`
                // камеры гасит ffmpeg. Поэтому ждать безопасно: кадр
                // читается блоками, а не держится вечно.
                let _ = session.worker.join();
                tracing::info!(
                    "захват камеры остановлен по команде, принято {} кадров",
                    session.frames.load(Ordering::Relaxed)
                );
            } else {
                tracing::debug!("команда остановки камеры: захвата не было");
            }
            self.clear_preview();
            self.lock(&self.status).device.clone()
        };
        self.set_stopped(&device, String::new());
        self.status()
    }

    /// Текущий кадр превью в виде, удобном интерфейсу.
    pub fn preview(&self) -> Option<CameraFrame> {
        // Под блокировкой только указатель: кодирование в base64 — самая
        // тяжёлая операция ответа, и держать на ней мьютекс нельзя, иначе
        // поток захвата не сможет положить свежий кадр.
        let frame = {
            let slot = self.preview.frame.lock().ok()?;
            Arc::clone(slot.as_ref()?)
        };
        Some(CameraFrame {
            width: frame.width,
            height: frame.height,
            rgb: frame.to_base64(),
        })
    }

    /// Последний кадр как есть: для детектора кистей.
    ///
    /// Отдаёт только указатель, без кодирования в base64: детектору нужны
    /// пиксели, а не строка для интерфейса.
    pub fn frame(&self) -> Option<Arc<RgbFrame>> {
        let slot = self.preview.frame.lock().ok()?;
        slot.as_ref().map(Arc::clone)
    }

    /// Кадр превью двоичным пакетом: заголовок и пиксели RGBA.
    ///
    /// Заголовок — ширина и высота по четыре байта. Пиксели уже разложены в
    /// RGBA: канвас в окне ждёт именно такой буфер, и раскладка здесь дешевле
    /// попиксельного цикла в интерфейсе, который тормозил бы отрисовку.
    pub fn packed_frame(&self) -> Option<Vec<u8>> {
        let frame = self.frame()?;
        let count = frame.width as usize * frame.height as usize;
        let mut out = Vec::with_capacity(8 + count * 4);
        out.extend_from_slice(&frame.width.to_le_bytes());
        out.extend_from_slice(&frame.height.to_le_bytes());
        for pixel in frame.pixels.chunks_exact(3) {
            out.push(pixel[0]);
            out.push(pixel[1]);
            out.push(pixel[2]);
            out.push(255);
        }
        Some(out)
    }

    /// Статус с числом кадров и ошибкой, набранной потоком.
    pub fn status(&self) -> CameraStatus {
        let session = self.lock(&self.session);
        self.status_of(session.as_ref())
    }

    /// Статус по уже взятой сессии.
    ///
    /// Отдельный метод не для красоты: `start` и `stop` держат мьютекс
    /// сессии, а `Mutex` не переиспользуется, поэтому повторный захват
    /// внутри них — это взаимоблокировка в одном потоке.
    fn status_of(&self, session: Option<&Session>) -> CameraStatus {
        let mut status = self
            .status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default();
        if let Some(session) = session {
            status.frames = session.frames.load(Ordering::Relaxed);
            if session.finished.load(Ordering::Relaxed) {
                status.running = false;
                status.error = self.lock(&session.error).clone().or(status.error.take());
            }
        }
        status
    }

    /// Убирает кадр превью: камера выключена или только что перезапущена.
    fn clear_preview(&self) {
        if let Ok(mut frame) = self.preview.frame.lock() {
            *frame = None;
        }
    }

    /// Отмечает камеру включённой.
    fn set_running(&self, device: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.running = true;
            status.device = device.to_string();
            status.error = None;
        }
    }

    /// Отмечает камеру остановленной, с причиной, если она есть.
    fn set_stopped(&self, device: &str, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.running = false;
            if !device.is_empty() {
                status.device = device.to_string();
            }
            status.error = if error.is_empty() { None } else { Some(error) };
        }
    }

    /// Берёт мьютекс, переживая панику в чужом потоке.
    ///
    /// Камера не хранит ничего, что нельзя потерять, поэтому после паники
    /// мьютекс берётся как есть: иначе одно падение в потоке кадров
    /// выключило бы камеру до перезапуска окна.
    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for CameraState {
    /// Останавливает камеру при выходе из окна.
    ///
    /// Без этого ffmpeg остался бы читать устройство после закрытия окна.
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;
