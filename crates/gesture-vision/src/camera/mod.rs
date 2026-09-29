//! Захват кадров с камеры через внешний процесс ffmpeg.
//!
//! Крейт не тащит ни видеозахват, ни кодеки: ffmpeg уже умеет открывать
//! устройства Video4Linux2 и отдавать кадры в нужном формате. Обёртка
//! только читает поток и нарезает его на кадры заданного размера.
//!
//! Модель детекции здесь не появляется. [`FrameSource`] отдаёт пиксели, а
//! точки кистей по-прежнему приходят из [`crate::LandmarkSource`]: когда
//! появится модель, она будет читать тот же [`RgbFrame`].
//!
//! ```text
//!   CameraConfig → ffmpeg → байты → FrameSource::next_frame → RgbFrame
//! ```

mod config;
mod frame;
mod source;
#[cfg(test)]
mod tests;

pub use config::CameraConfig;
pub use frame::RgbFrame;
pub use source::FfmpegCamera;

use gesture_core::PipelineError;

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
