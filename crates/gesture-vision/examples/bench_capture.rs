//! Временный стенд: сколько кадров в секунду держит конвейер Rust.
//!
//! Меряет отдельно стоимость resize и base64, потому что в окне
//! пользователь видит ~1-5 fps, а камера отдаёт 30. Нужно понять, где
//! именно теряется время, прежде чем добавлять потоки.

use std::time::{Duration, Instant};

use gesture_vision::camera::{CameraConfig, FrameSource};

const FRAMES: usize = 90;

fn bench(label: &str, config: CameraConfig, resize: Option<(u32, u32)>, b64: bool) {
    let mut camera = match gesture_vision::camera::FfmpegCamera::start(config) {
        Ok(camera) => camera,
        Err(err) => {
            println!("{label}: НЕ ЗАПУСТИЛОСЬ: {err}");
            return;
        }
    };

    let mut frames = 0usize;
    let mut resize_time = Duration::ZERO;
    let mut b64_time = Duration::ZERO;
    let mut b64_bytes = 0usize;

    let start = Instant::now();
    while frames < FRAMES {
        let frame = match camera.next_frame() {
            Ok(frame) => frame,
            Err(err) => {
                println!("{label}: ОШИБКА на кадре {frames}: {err}");
                break;
            }
        };
        frames += 1;

        if let Some((w, h)) = resize {
            let t = Instant::now();
            let _small = frame.resized(w, h);
            resize_time += t.elapsed();
        }
        if b64 {
            let t = Instant::now();
            let encoded: String = frame.to_base64();
            b64_time += t.elapsed();
            b64_bytes = encoded.len();
        }
    }
    let elapsed = start.elapsed();

    let fps = if elapsed.as_secs_f64() > 0.0 {
        frames as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };
    let r_ms = resize_time.as_secs_f64() * 1000.0;
    let b_ms = b64_time.as_secs_f64() * 1000.0;
    let wall = elapsed.as_secs_f64() * 1000.0;

    println!(
        "{label}\n  кадров: {frames}  итог: {fps:.1} fps  (стена {wall:.0} мс)\n  \
         resize: {r_ms:.0} мс суммарно ({:.2} мс/кадр)  base64: {b_ms:.0} мс суммарно ({:.2} мс/кадр, {b64_bytes} байт)\n  \
         прочее (чтение+копирование): {:.0} мс",
        r_ms / frames as f64,
        b_ms / frames as f64,
        (wall - r_ms - b_ms).max(0.0),
    );
}

fn cfg(w: u32, h: u32, fps: u32) -> CameraConfig {
    CameraConfig {
        device: std::path::PathBuf::from("/dev/video0"),
        size: (w, h),
        fps,
    }
}

fn main() {
    // Держим простой вывод: печатаем построчно, без переносов строк вшитую строку.
    println!("=== конвейер Rust, до {} кадров на вариант ===\n", FRAMES);

    bench(
        "A: текущий код — 640x480, resize 320x180, base64, fps=15",
        cfg(640, 480, 15),
        Some((320, 180)),
        true,
    );
    bench(
        "B: 320x180 от ffmpeg, base64, fps=30",
        cfg(320, 180, 30),
        None,
        true,
    );
    bench(
        "C: 320x180 от ffmpeg, без base64, fps=30",
        cfg(320, 180, 30),
        None,
        false,
    );
    bench(
        "D: 640x480, без resize/base64, fps=30",
        cfg(640, 480, 30),
        None,
        false,
    );
    bench(
        "E: 640x480, resize 320x180, без base64, fps=30",
        cfg(640, 480, 30),
        Some((320, 180)),
        false,
    );
}
