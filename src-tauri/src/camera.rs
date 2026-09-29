//! Живой захват камеры: фоновый поток, счётчики и последний кадр.
//!
//! Захват идёт всегда, пока камера включена, а разбор кадра — только там,
//! где есть модель. Сейчас модели нет, поэтому поток лишь копит кадры для
//! превью и честно считает их: интерфейсу нужно видеть, что камера жива,
//! даже когда распознавание ещё не подключено.
//!
//! Поток кадров и команды не делят мьютекс: превью лежит в отдельном
//! `Arc`, иначе чтение кадра задерживало бы ответы интерфейса.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use gesture_vision::{CameraConfig, FfmpegCamera, FrameSource, RgbFrame};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Событие со сменой состояния камеры: включилась, остановилась, ошибка.
pub const CAMERA_EVENT: &str = "gesture://camera";

/// Размер превью.
///
/// Меньше исходного кадра по двум причинам: кадр уходит в интерфейс как
/// base64 в JSON-событии, и большой кадр на каждый кадр камеры забьёт
/// очередь IPC. 320x180 RGB24 — это 172 800 байт на кадр.
const PREVIEW: (u32, u32) = (320, 180);

/// Запрос на включение камеры.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraRequest {
    /// Устройство Video4Linux2. Пусто — взять значение по умолчанию.
    pub device: String,
    /// Ширина кадра в пикселях. `0` — значение по умолчанию.
    pub width: u32,
    /// Высота кадра в пикселях. `0` — значение по умолчанию.
    pub height: u32,
    /// Частота кадров. `0` — значение по умолчанию.
    pub fps: u32,
}

impl CameraRequest {
    /// Превращает запрос в параметры захвата, подставляя умолчания.
    ///
    /// Пустые поля отличают «не задано» от «ноль», поэтому подстановка
    /// идёт по полям, а не одним сравнением всего запроса.
    pub fn config(&self) -> CameraConfig {
        let fallback = CameraConfig::default();
        CameraConfig {
            device: if self.device.trim().is_empty() {
                fallback.device
            } else {
                self.device.trim().into()
            },
            size: (
                if self.width == 0 {
                    fallback.size.0
                } else {
                    self.width
                },
                if self.height == 0 {
                    fallback.size.1
                } else {
                    self.height
                },
            ),
            fps: if self.fps == 0 { fallback.fps } else { self.fps },
        }
    }
}

/// Состояние камеры для интерфейса.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraStatus {
    /// Идёт ли захват прямо сейчас.
    pub running: bool,
    /// Устройство, из которого идёт или шёл захват.
    pub device: String,
    /// Сколько кадров прочитано за время работы камеры.
    pub frames: u64,
    /// Почему камера остановилась, если остановилась сама.
    pub error: Option<String>,
}

/// Кадр для интерфейса: уменьшенная копия в base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraFrame {
    /// Ширина картинки в пикселях.
    pub width: u32,
    /// Высота картинки в пикселях.
    pub height: u32,
    /// Пиксели RGB24 в base64, три байта на точку.
    pub rgb: String,
}

/// Статус и кадр одним ответом: интерфейс опрашивает и то и сразу.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraFrameResult {
    /// Состояние камеры на момент кадра.
    pub status: CameraStatus,
    /// Кадр превью, если камера уже успела его прочитать.
    pub frame: Option<CameraFrame>,
}

/// Последний уменьшенный кадр, общий для команд и потока кадров.
#[derive(Debug, Clone)]
struct Preview {
    /// Кадр под мьютексом: он большой, и отдавать его наружу нельзя.
    frame: Arc<Mutex<Option<RgbFrame>>>,
}

/// Состояние захвата, разделяемое между потоком камеры и командами.
#[derive(Debug)]
struct Session {
    /// Поток остановлен: команды и выход из окна просят его завершиться.
    stop: Arc<AtomicBool>,
    /// Поток дошёл до конца сам, без команды остановки.
    finished: Arc<AtomicBool>,
    /// Счётчик прочитанных кадров.
    frames: Arc<AtomicU64>,
    /// Почему поток остановился, если остановился сам.
    error: Arc<Mutex<Option<String>>>,
    /// Поток захвата.
    worker: JoinHandle<()>,
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
            self.set_stopped(&device, error.to_string());
            return self.status();
        }
        let mut camera = match FfmpegCamera::start(config) {
            Ok(camera) => camera,
            Err(error) => {
                drop(slot);
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
                .spawn(move || capture_loop(&mut camera, stop, finished, frames, error, preview))
        };
        let worker = match worker {
            Ok(worker) => worker,
            Err(source) => {
                drop(slot);
                self.set_stopped(&device, format!("не удалось создать поток камеры: {source}"));
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
            }
            self.clear_preview();
            self.lock(&self.status).device.clone()
        };
        self.set_stopped(&device, String::new());
        self.status()
    }

    /// Текущий кадр превью в виде, удобном интерфейсу.
    pub fn preview(&self) -> Option<CameraFrame> {
        let frame = self.preview.frame.lock().ok()?;
        let frame = frame.as_ref()?;
        Some(CameraFrame {
            width: frame.width,
            height: frame.height,
            rgb: frame.to_base64(),
        })
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
                status.error = self
                    .lock(&session.error)
                    .clone()
                    .or(status.error.take());
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
            status.error = if error.is_empty() {
                None
            } else {
                Some(error)
            };
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

/// Читает кадры, пока не попросят остановиться.
fn capture_loop(
    camera: &mut FfmpegCamera,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
    preview: Preview,
) {
    while !stop.load(Ordering::Relaxed) {
        match camera.next_frame() {
            Ok(frame) => {
                frames.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut slot) = preview.frame.lock() {
                    // Меньшая копия вместо полного кадра: превью не нуждается
                    // в пикселях, а держать их в памяти между кадрами незачем.
                    *slot = Some(frame.resized(PREVIEW.0, PREVIEW.1));
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

impl Drop for CameraState {
    /// Останавливает камеру при выходе из окна.
    ///
    /// Без этого ffmpeg остался бы читать устройство после закрытия окна.
    fn drop(&mut self) {
        self.stop();
    }
}

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
pub fn stop_camera(
    app: tauri::AppHandle,
    camera: tauri::State<'_, CameraState>,
) -> CameraStatus {
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// Сколько тест вправе длиться.
    ///
    /// Камера читает кадры блоками, поэтому зависший тест — это не «медленно»,
    /// а взаимоблокировка, которую иначе приходится искать вручную.
    const LIMIT: Duration = Duration::from_secs(20);

    /// Сигнал о конце потока, который срабатывает и при панике.
    ///
    /// Обычный `send` в конце тела не годится: упавшая проверка молчала бы
    /// до конца таймаута, и настоящая причина пряталась бы за «зависло».
    struct Done(mpsc::Sender<()>);

    impl Drop for Done {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    /// Запускает проверку в отдельном потоке с пределом времени.
    ///
    /// Поток нужен из-за одной причины: зависший `join` внутри камеры
    /// иначе держит весь бинарник и не даёт увидеть, какой тест виноват.
    fn within<F>(name: &str, body: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let (done, ready) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name(format!("test-{name}"))
            .spawn(move || {
                let _done = Done(done);
                body();
            })
            .expect("не удалось создать поток теста");
        if ready.recv_timeout(LIMIT).is_err() {
            // Отпускаем поток: тест провалится, но бинарник не зависнет.
            let _ = worker.join();
            panic!("{name}: тест не уложился в {LIMIT:?}");
        }
        // Паника внутри потока всплывает здесь, с исходным сообщением.
        worker.join().expect("поток теста упал");
    }

    #[test]
    fn request_uses_defaults_for_empty_fields() {
        within("request_uses_defaults_for_empty_fields", || {
            let config = CameraRequest::default().config();
            assert_eq!(config.device.to_string_lossy(), "/dev/video0");
            assert_eq!(config.size, (640, 480));
            assert_eq!(config.fps, 15);
        });
    }

    #[test]
    fn request_keeps_given_values() {
        within("request_keeps_given_values", || {
            let request = CameraRequest {
                device: " /dev/video1 ".into(),
                width: 320,
                height: 240,
                fps: 30,
            };
            let config = request.config();
            assert_eq!(config.device.to_string_lossy(), "/dev/video1");
            assert_eq!(config.size, (320, 240));
            assert_eq!(config.fps, 30);
        });
    }

    #[test]
    fn request_keeps_half_given_size() {
        within("request_keeps_half_given_size", || {
            // Ширина задана, высота нет: подставляется только высота.
            let request = CameraRequest {
                width: 1280,
                ..CameraRequest::default()
            };
            assert_eq!(request.config().size, (1280, 480));
        });
    }

    #[test]
    fn zero_width_keeps_default_width() {
        within("zero_width_keeps_default_width", || {
            let request = CameraRequest {
                height: 240,
                ..CameraRequest::default()
            };
            assert_eq!(request.config().size, (640, 240));
        });
    }

    #[test]
    fn stop_on_fresh_camera_is_harmless() {
        within("stop_on_fresh_camera_is_harmless", || {
            let state = CameraState::new();
            let status = state.stop();
            assert!(!status.running);
            assert!(status.error.is_none());
            assert!(state.preview().is_none());
        });
    }

    #[test]
    fn status_starts_off() {
        within("status_starts_off", || {
            let status = CameraState::new().status();
            assert!(!status.running);
            assert!(status.device.is_empty());
            assert_eq!(status.frames, 0);
        });
    }

    #[test]
    fn missing_device_is_reported_without_panicking() {
        within("missing_device_is_reported_without_panicking", || {
            // Настоящий ffmpeg в тест зовём: он честно падает на неверном
            // пути, и проверка стоит того — именно этот путь видит человек.
            let state = CameraState::new();
            let request = CameraRequest {
                device: "/dev/видео-нет-такого".into(),
                ..CameraRequest::default()
            };
            let status = wait_for_error(&state, &request);
            assert!(!status.running);
            assert!(status.error.is_some(), "причина остановки должна быть видна");
            assert_eq!(status.frames, 0);
        });
    }

    #[test]
    fn restart_after_failure_spawns_new_process() {
        within("restart_after_failure_spawns_new_process", || {
            // Разные пути в двух попытках: по тексту ошибки видно, что
            // вторая сессия действительно пошла, а не вернула старый статус.
            let state = CameraState::new();
            let first = CameraRequest {
                device: "/dev/видео-нет-такого".into(),
                ..CameraRequest::default()
            };
            let second = CameraRequest {
                device: "/dev/и-этого-тоже-нет".into(),
                ..CameraRequest::default()
            };
            let failed = wait_for_error(&state, &first);
            assert!(failed.device.contains("нет-такого"));
            let restarted = wait_for_error(&state, &second);
            assert!(
                restarted.device.contains("и-этого-тоже-нет"),
                "перезапуск должен открыть новое устройство, а не старое: {restarted:?}"
            );
            let reason = restarted.error.expect("причина должна быть видна");
            assert!(
                reason.contains("и-этого-тоже-нет"),
                "ошибка должна называть новое устройство: {reason}"
            );
        });
    }

    #[test]
    fn real_camera_delivers_frames_when_device_exists() {
        within("real_camera_delivers_frames_when_device_exists", || {
            // Тест про настоящее железо: без него весь остальной набор
            // говорит только о том, что код собирается, а не работает.
            if !std::path::Path::new("/dev/video0").exists() {
                eprintln!("пропуск: нет /dev/video0");
                return;
            }
            let state = CameraState::new();
            let request = CameraRequest {
                device: "/dev/video0".into(),
                ..CameraRequest::default()
            };
            let started = state.start(&request);
            assert!(started.error.is_none(), "старт не должен падать: {started:?}");

            // Ждём первый кадр, а не фиксированную паузу: скорость камеры
            // неизвестна, а sleep здесь означал бы либо ожидание впустую,
            // либо flakes на медленной машине.
            let deadline = std::time::Instant::now() + LIMIT;
            let status = loop {
                let status = state.status();
                if status.frames > 0 || !status.running {
                    break status;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "кадр не пришёл за {LIMIT:?}: {status:?}"
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            assert!(status.running, "камера должна работать: {status:?}");
            assert!(status.error.is_none(), "ошибок быть не должно: {status:?}");

            let frame = state.preview().expect("кадр должен появиться в превью");
            assert_eq!(frame.width, 320);
            assert_eq!(frame.height, 180);
            // base64 кодирует по 3 байта в 4 символа, с хвостом из '='.
            let bytes = frame.width as usize * frame.height as usize * 3;
            let expected = bytes.div_ceil(3) * 4;
            assert_eq!(
                frame.rgb.len(),
                expected,
                "превью должно нести весь кадр, а не обрезок"
            );
            state.stop();
        });
    }

    /// Ждёт, пока поток камеры сам доедет до ошибки, и возвращает статус.
    ///
    /// Поток читает кадр блоками, поэтому сразу после `start` статус ещё
    /// может быть «работает». Ждать нужно на условии, а не на таймере.
    fn wait_for_error(state: &CameraState, request: &CameraRequest) -> CameraStatus {
        state.start(request);
        let deadline = std::time::Instant::now() + LIMIT;
        loop {
            let status = state.status();
            if !status.running && status.error.is_some() {
                return status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "камера не сообщила об ошибке за {LIMIT:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
