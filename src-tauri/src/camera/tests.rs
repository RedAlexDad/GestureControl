//! Тесты состояния камеры и подстановки настроек.
//!
//! Живое железо здесь тоже проверяется: без такого теста набор говорил бы
//! только о том, что код собирается, а не работает.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::config::Settings;

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
        // Пустой запрос обязан взять общие настройки, а не жёсткие
        // числа из этого модуля: иначе `.env` перестал бы влиять на камеру.
        let config = CameraRequest::default().config();
        let settings = Settings::global();
        assert_eq!(config.device, PathBuf::from(&settings.camera.device));
        assert_eq!(config.size, (settings.camera.width, settings.camera.height));
        assert_eq!(config.fps, settings.camera.fps);
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
        assert!(
            status.error.is_some(),
            "причина остановки должна быть видна"
        );
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
        assert!(
            started.error.is_none(),
            "старт не должен падать: {started:?}"
        );

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
