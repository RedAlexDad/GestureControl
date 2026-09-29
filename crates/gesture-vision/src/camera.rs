//! Захват кадров с камеры через внешний процесс ffmpeg.
//!
//! Крейт не тащит ни видеозахват, ни кодеки: ffmpeg уже умеет открывать
//! устройства Video4Linux2 и отдавать кадры в нужном формате. Обёртка
//! только читает поток и нарезает его на кадры заданного размера.
//!
//! Модель детекции здесь не появляется. [`FrameSource`] отдаёт пиксели, а
//! точки кистей по-прежнему приходят из [`crate::LandmarkSource`]: когда
//! появится модель, она будет читать тот же [`RgbFrame`].

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

use gesture_core::PipelineError;

/// Алфавит base64 без переводов строк.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Кадр камеры: пиксели RGB24, три байта на точку, в порядке слева направо.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbFrame {
    /// Ширина кадра в пикселях.
    pub width: u32,
    /// Высота кадра в пикселях.
    pub height: u32,
    /// Пиксели RGB24: ровно `width * height * 3` байт.
    pub pixels: Vec<u8>,
}

impl RgbFrame {
    /// Кадр заданного размера без единого заполненного пикселя.
    pub fn empty(width: u32, height: u32) -> Self {
        RgbFrame {
            width,
            height,
            pixels: vec![0; frame_bytes(width, height)],
        }
    }

    /// Кадр из готовых пикселей: длина проверяется, лишнее отбрасывается.
    ///
    /// Нужно тестам и источникам, которые считают кадр целиком, а не по
    /// частям: тихо принять кадр неверной длины означает потом искать
    /// причину сдвига картинки вместо того, чтобы остановиться на ошибке.
    pub fn new(width: u32, height: u32, mut pixels: Vec<u8>) -> Result<Self, PipelineError> {
        let expected = frame_bytes(width, height);
        if expected == 0 {
            return Err(PipelineError::Camera(format!(
                "некорректный размер кадра {width}x{height}"
            )));
        }
        pixels.truncate(expected);
        pixels.resize(expected, 0);
        Ok(RgbFrame {
            width,
            height,
            pixels,
        })
    }

    /// Меньшая копия кадра для превью.
    ///
    /// Ближайший сосед, а не усреднение: превью нужно дёшево, а размытие
    /// на нём только мешает разглядеть контуры кисти.
    pub fn resized(&self, width: u32, height: u32) -> RgbFrame {
        let width = width.clamp(1, self.width);
        let height = height.clamp(1, self.height);
        if width == self.width && height == self.height {
            return self.clone();
        }
        let mut pixels = vec![0; frame_bytes(width, height)];
        for y in 0..height {
            let source_row = (y as u64 * self.height as u64 / height as u64) as usize;
            for x in 0..width {
                let source_column = (x as u64 * self.width as u64 / width as u64) as usize;
                let from = (source_row * self.width as usize + source_column) * 3;
                let to = (y as usize * width as usize + x as usize) * 3;
                pixels[to..to + 3].copy_from_slice(&self.pixels[from..from + 3]);
            }
        }
        RgbFrame {
            width,
            height,
            pixels,
        }
    }

    /// Пиксели в base64: так интерфейс получает кадр без кодека на стороне
    /// окна и рисует его через `ImageData`.
    pub fn to_base64(&self) -> String {
        base64_encode(&self.pixels)
    }
}

/// Сколько байт занимает кадр RGB24 такого размера.
fn frame_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * 3
}

/// Кодирует байты в base64 со стандартным заполнением.
fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64[(triple >> 18) as usize & 0x3f] as char);
        out.push(BASE64[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            BASE64[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Источник кадров камеры.
pub trait FrameSource {
    /// Следующий кадр.
    ///
    /// Ошибка означает, что источник сломался и больше кадров не будет:
    /// вызывающий закрывает камеру и сообщает пользователю причину.
    fn next_frame(&mut self) -> Result<RgbFrame, PipelineError>;

    /// Размер кадров, которые вернёт источник.
    fn size(&self) -> (u32, u32);
}

/// Параметры захвата камеры.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraConfig {
    /// Устройство Video4Linux2, например `/dev/video0`.
    pub device: PathBuf,
    /// Размер кадра в пикселях.
    pub size: (u32, u32),
    /// Сколько кадров в секунду нужно источнику.
    pub fps: u32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        CameraConfig {
            device: PathBuf::from("/dev/video0"),
            size: (640, 480),
            fps: 15,
        }
    }
}

impl CameraConfig {
    /// Проверяет параметры до запуска процесса.
    ///
    /// Размер нулевой в пикселях превращает чтение кадра в падение на
    /// делении, поэтому такие значения отсекаются сразу.
    pub fn validated(&self) -> Result<(), PipelineError> {
        let (width, height) = self.size;
        if width == 0 || height == 0 {
            return Err(PipelineError::Camera(format!(
                "некорректный размер кадра {width}x{height}"
            )));
        }
        if self.fps == 0 {
            return Err(PipelineError::Camera(format!(
                "некорректная частота кадров {}",
                self.fps
            )));
        }
        Ok(())
    }

    /// Команда ffmpeg, которая отдаёт кадры RGB24 в stdout.
    fn command(&self) -> Command {
        let (width, height) = self.size;
        let mut command = Command::new("ffmpeg");
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(["-f", "v4l2", "-i"])
            .arg(&self.device)
            // Размер и частоту задаём на своей стороне: камера может не
            // уметь ни один из своих режимов, а ffmpeg пересчитает кадр
            // в нужный и отдаст ровно столько пикселей, сколько мы ждём.
            .args(["-vf", &format!("scale={width}:{height},fps={}", self.fps)])
            .args(["-pix_fmt", "rgb24", "-f", "rawvideo", "-"]);
        command
    }
}

/// Камера, читающая кадры из процесса ffmpeg.
#[derive(Debug)]
pub struct FfmpegCamera {
    config: CameraConfig,
    child: Child,
    stdout: ChildStdout,
    /// Кадр целиком, пока не собран: ffmpeg отдаёт поток без границ.
    frame: Vec<u8>,
    /// Что сказал ffmpeg перед остановкой, для внятной ошибки.
    failure: Option<String>,
}

impl FfmpegCamera {
    /// Запускает захват с параметрами по умолчанию для устройства.
    pub fn start(config: CameraConfig) -> Result<Self, PipelineError> {
        let command = config.command();
        FfmpegCamera::spawn(config, command)
    }

    /// Запускает уже собранную команду.
    ///
    /// Отдельный путь нужен тестам: они подставляют процесс, который
    /// печатает заранее заданные байты, и не требуют камеры.
    fn spawn(config: CameraConfig, mut command: Command) -> Result<Self, PipelineError> {
        config.validated()?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PipelineError::Camera("ffmpeg не отдал поток кадров".into()))?;
        let (width, height) = config.size;
        Ok(FfmpegCamera {
            frame: vec![0; frame_bytes(width, height)],
            config,
            child,
            stdout,
            failure: None,
        })
    }

    /// Устройство, из которого идёт захват.
    pub fn device(&self) -> &PathBuf {
        &self.config.device
    }
}

impl FrameSource for FfmpegCamera {
    fn next_frame(&mut self) -> Result<RgbFrame, PipelineError> {
        if let Some(message) = &self.failure {
            return Err(PipelineError::Camera(message.clone()));
        }
        match self.stdout.read_exact(&mut self.frame) {
            Ok(()) => RgbFrame::new(self.config.size.0, self.config.size.1, self.frame.clone()),
            Err(error) => {
                let message = self.explain_exit(&error);
                self.failure = Some(message.clone());
                Err(PipelineError::Camera(message))
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        self.config.size
    }
}

impl FfmpegCamera {
    /// Объясняет остановку источника.
    ///
    /// ffmpeg пишет причину в stderr и уходит, поэтому его последнее
    /// сообщение полезнее сообщения о неожиданном конце файла.
    fn explain_exit(&mut self, error: &std::io::Error) -> String {
        let mut reason = String::new();
        if let Some(mut stderr) = self.child.stderr.take() {
            let mut buffer = Vec::new();
            let _ = stderr.read_to_end(&mut buffer);
            reason = String::from_utf8_lossy(&buffer).trim().to_string();
        }
        // Многострочный лог ffmpeg не нужен пользователю: в интерфейс уходит
        // строка уведомления, а полный текст остаётся в журнале.
        let reason = reason.lines().take(3).collect::<Vec<_>>().join(" ");
        if reason.is_empty() {
            format!("ffmpeg остановился: {error}")
        } else {
            format!("ffmpeg остановился: {reason}")
        }
    }
}

impl Drop for FfmpegCamera {
    fn drop(&mut self) {
        // Процесс не должен пережить камеру: иначе ffmpeg остаётся висеть
        // и держит устройство занятым после выключения. Ошибка `kill` здесь
        // ничего не меняет: ребёнок либо уже ушёл сам, либо его нечем убить.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
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
        let mut config = CameraConfig::default();
        config.fps = 0;
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
}
