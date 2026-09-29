use std::path::PathBuf;
use std::process::Command;

use gesture_core::PipelineError;

use super::frame::base64_encode;
use super::CameraConfig;
use super::*;

/// Путь, уникальный для одного вызова.
///
/// Тесты идут параллельно, а кадры лежат в файле: общий путь приводит
/// к тому, что один тест читает чужой кадр.
fn temp_path(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "gesture-vision-{}-{unique}-{tag}",
        std::process::id()
    ))
}

/// Источник из заданных кадров: файл с байтами и команда `cat`.
///
/// Настоящий ffmpeg в тестах не нужен: проверяется нарезка потока на
/// кадры фиксированного размера, а она одинакова для любого источника
/// байтов нужной длины.
fn source_of_frames(frames: &[Vec<u8>]) -> StubSource {
    let path = temp_path("frames.bin");
    let mut data = Vec::new();
    for frame in frames {
        data.extend_from_slice(frame);
    }
    std::fs::write(&path, data).expect("запись файла с кадрами");
    StubSource {
        path,
        config: CameraConfig {
            device: PathBuf::from("/dev/null"),
            size: (2, 1),
            fps: 1,
        },
    }
}

/// Подставной источник вместе с файлом, который нужно убрать за собой.
struct StubSource {
    path: PathBuf,
    config: CameraConfig,
}

impl StubSource {
    /// Запускает источник на команде, печатающей подготовленные кадры.
    fn open(&self) -> Result<FfmpegCamera, PipelineError> {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("cat {}", self.path.display()));
        FfmpegCamera::spawn(self.config.clone(), command)
    }

    /// Убирает временный файл.
    fn cleanup(&self) {
        std::fs::remove_file(&self.path).ok();
    }
}

/// Кадр из двух пикселей с различимыми значениями.
fn frame(first: u8, second: u8) -> Vec<u8> {
    vec![first, 0, 0, second, 0, 0]
}

#[test]
fn reads_frames_in_order() {
    let source = source_of_frames(&[frame(1, 2), frame(3, 4)]);
    let mut camera = source.open().expect("источник запустился");
    let first = camera.next_frame().expect("первый кадр");
    let second = camera.next_frame().expect("второй кадр");
    source.cleanup();
    assert_eq!(first.pixels, frame(1, 2));
    assert_eq!(second.pixels, frame(3, 4));
    assert_eq!(camera.size(), (2, 1));
}

#[test]
fn reports_failure_after_source_ends() {
    let source = source_of_frames(&[frame(1, 2)]);
    let mut camera = source.open().expect("источник запустился");
    camera.next_frame().expect("единственный кадр");
    let error = camera
        .next_frame()
        .expect_err("после конца потока нужна ошибка");
    source.cleanup();
    assert!(matches!(error, PipelineError::Camera(_)));
    // Ошибка запоминается: повторный вызов не должен заново читать поток.
    let again = camera.next_frame().expect_err("ошибка повторяется");
    assert_eq!(error.to_string(), again.to_string());
}

#[test]
fn explains_failure_with_reported_reason() {
    let source = StubSource {
        path: temp_path("missing.bin"),
        config: CameraConfig {
            device: PathBuf::from("/dev/null"),
            size: (2, 1),
            fps: 1,
        },
    };
    let mut command = Command::new("sh");
    command.arg("-c").arg("cat /nonexistent/frames.bin; exit 1");
    let mut camera =
        FfmpegCamera::spawn(source.config.clone(), command).expect("источник запустился");
    let error = camera.next_frame().expect_err("поток сразу кончился");
    let text = error.to_string();
    assert!(text.contains("ffmpeg"), "нет имени процесса: {text}");
    assert!(
        text.contains("No such file") || text.contains("не найден"),
        "нет причины от ffmpeg: {text}"
    );
}

#[test]
fn missing_program_is_reported_as_camera_error() {
    let config = CameraConfig {
        device: PathBuf::from("/dev/null"),
        size: (2, 1),
        fps: 1,
    };
    let command = Command::new("/nonexistent/ffmpeg");
    let error = FfmpegCamera::spawn(config, command).expect_err("нет такой программы");
    assert!(matches!(error, PipelineError::Camera(_)));
}

#[test]
fn zero_size_is_rejected_before_spawn() {
    let config = CameraConfig {
        device: PathBuf::from("/dev/null"),
        size: (0, 480),
        fps: 15,
    };
    let command = Command::new("sh");
    let error = FfmpegCamera::spawn(config, command).expect_err("нулевая ширина");
    assert!(error.to_string().contains("некорректный размер"));
}

#[test]
fn zero_fps_is_rejected() {
    let config = CameraConfig {
        fps: 0,
        ..CameraConfig::default()
    };
    assert!(config.validated().is_err());
}

#[test]
fn short_frame_is_padded_to_full_size() {
    let cropped = RgbFrame::new(2, 1, vec![1, 2, 3]).expect("короткий кадр");
    assert_eq!(cropped.pixels.len(), 6);
}

#[test]
fn zero_size_frame_is_rejected() {
    assert!(RgbFrame::new(0, 0, vec![]).is_err());
}

#[test]
fn resized_keeps_proportions_by_nearest_neighbour() {
    let source = RgbFrame::new(4, 2, vec![1; 24]).expect("исходный кадр");
    let smaller = source.resized(2, 1);
    assert_eq!((smaller.width, smaller.height), (2, 1));
    assert_eq!(smaller.pixels.len(), 6);
    // Больший запрос, чем есть, не выходит за исходный размер.
    let bigger = source.resized(64, 64);
    assert_eq!((bigger.width, bigger.height), (4, 2));
}

#[test]
fn resized_pads_wide_target_instead_of_stretching() {
    // Кадр 4:3 в окно 2:1: картинка остаётся 4:3 и по краям идут чёрные
    // поля. Без этого кадр растягивался бы вдвое по ширине.
    let mut source = RgbFrame::new(4, 3, vec![0; 36]).expect("исходный кадр");
    for (index, pixel) in source.pixels.chunks_mut(3).enumerate() {
        pixel.copy_from_slice(&[index as u8, 1, 2]);
    }
    let preview = source.resized(4, 2);
    assert_eq!((preview.width, preview.height), (4, 2));
    let pixel_at = |x: usize, y: usize| {
        let from = (y * preview.width as usize + x) * 3;
        preview.pixels[from..from + 3].to_vec()
    };
    // Верхняя строка исходника целиком, дальше поля слева и справа.
    assert_eq!(pixel_at(0, 0), vec![0, 1, 2]);
    assert_eq!(pixel_at(1, 0), vec![1, 1, 2]);
    assert_eq!(pixel_at(2, 0), vec![2, 1, 2]);
    assert_eq!(pixel_at(3, 0), vec![0, 0, 0]);
    // Вторая строка исходника, а не первая повторённая.
    assert_eq!(pixel_at(0, 1), vec![4, 1, 2]);
    assert_eq!(pixel_at(3, 1), vec![0, 0, 0]);
}

#[test]
fn resized_pads_tall_target_instead_of_stretching() {
    // Квадратный кадр в широкое окно: остаётся квадратом, поля по бокам
    // закрыты чёрным, а не растянутыми полосами кадра.
    let source = RgbFrame::new(4, 4, vec![7; 48]).expect("исходный кадр");
    let preview = source.resized(4, 2);
    assert_eq!((preview.width, preview.height), (4, 2));
    let column = |x: usize| {
        let from = x * 3;
        preview.pixels[from..from + 3].to_vec()
    };
    assert_eq!(column(0), vec![0, 0, 0]);
    assert_eq!(column(1), vec![7, 7, 7]);
    assert_eq!(column(2), vec![7, 7, 7]);
    assert_eq!(column(3), vec![0, 0, 0]);
}

#[test]
fn resized_picks_expected_pixels() {
    let mut source = RgbFrame::new(2, 1, vec![0; 6]).expect("исходный кадр");
    source.pixels = vec![10, 0, 0, 20, 0, 0];
    let smaller = source.resized(1, 1);
    assert_eq!(smaller.pixels, vec![10, 0, 0]);
}

#[test]
fn base64_matches_known_vectors() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    assert_eq!(base64_encode(b"hi"), "aGk=");
    assert_eq!(base64_encode(b"abc"), "YWJj");
}

#[test]
fn default_config_targets_first_video_device() {
    let config = CameraConfig::default();
    assert_eq!(config.device, PathBuf::from("/dev/video0"));
    assert!(config.validated().is_ok());
}
