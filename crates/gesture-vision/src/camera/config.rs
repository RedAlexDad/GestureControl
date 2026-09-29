//! Параметры захвата: устройство, размер, частота.

use std::path::PathBuf;
use std::process::Command;

use gesture_core::PipelineError;

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
    pub(super) fn command(&self) -> Command {
        let (width, height) = self.size;
        let mut command = Command::new("ffmpeg");
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            // Размер просим у самой камеры: если задать его через `scale`,
            // камера отдаст свой режим (часто 16:9), а фильтр сожмёт его в
            // 4:3 — картинка получится перетянутой. Устройство само умеет
            // 640x480, поэтому масштабирование не нужно вовсе.
            .args([
                "-f",
                "v4l2",
                "-video_size",
                &format!("{width}x{height}"),
                "-i",
            ])
            .arg(&self.device)
            // Частоту оставляем фильтром: даже если камера не отдаёт ровно
            // столько кадров, поток уходит размеренным.
            .args(["-vf", &format!("fps={}", self.fps)])
            .args(["-pix_fmt", "rgb24", "-f", "rawvideo", "-"]);
        command
    }
}
