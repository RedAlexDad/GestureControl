//! Источник кадров поверх процесса ffmpeg.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

use gesture_core::PipelineError;

use super::config::CameraConfig;
use super::frame::{frame_bytes, RgbFrame};
use super::FrameSource;

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
    pub(super) fn spawn(config: CameraConfig, mut command: Command) -> Result<Self, PipelineError> {
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

impl Drop for FfmpegCamera {
    fn drop(&mut self) {
        // Процесс не должен пережить камеру: иначе ffmpeg остаётся висеть
        // и держит устройство занятым после выключения. Ошибка `kill` здесь
        // ничего не меняет: ребёнок либо уже ушёл сам, либо его нечем убить.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
