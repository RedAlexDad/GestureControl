//! Источник времени для моста.
//!
//! Мост не должен зависеть от настенных часов: в тестах время двигается
//! пошагово, а в приложении его задаёт камера или системный монотонный
//! счётчик. Оба случая закрывает один трейт.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Момент времени в секундах.
pub type Seconds = f32;

/// Источник времени.
///
/// Трейт объектно-безопасен (`Send + Sync`), потому что мост живёт в
/// общем состоянии окна и читается из нескольких потоков.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Текущее время в секундах.
    fn now(&self) -> Seconds;
}

/// Системные часы: отсчёт от запуска окна.
///
/// Отсчёт от старта, а не `SystemTime`, потому что распознавание интересует
/// только интервалы между кадрами, а не абсолютное время.
#[derive(Debug)]
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    /// Создаёт часы с отсчётом от текущего момента.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Seconds {
        self.origin.elapsed().as_secs_f32()
    }
}

/// Время, которое двигают вручную: для тестов и воспроизведения кадров.
#[derive(Debug)]
pub struct ManualClock {
    time: AtomicU64,
}

impl ManualClock {
    /// Создаёт часы, остановленные в нуле.
    pub fn new() -> Self {
        Self {
            time: AtomicU64::new(0),
        }
    }

    /// Создаёт часы, остановленные в заданный момент.
    pub fn starting_at(time: Seconds) -> Self {
        Self {
            time: AtomicU64::new(to_millis(time)),
        }
    }

    /// Сдвигает время вперёд и возвращает новое значение.
    ///
    /// Время не идёт назад: кадр с устаревшей меткой не должен откатывать
    /// обратный отсчёт или продвигать буфер динамики.
    pub fn advance(&self, delta: Seconds) -> Seconds {
        let delta = to_millis(delta.max(0.0));
        self.time.fetch_add(delta, Ordering::SeqCst);
        self.time.load(Ordering::SeqCst) as f32 / 1000.0
    }

    /// Возвращает текущее время без сдвига.
    pub fn current(&self) -> Seconds {
        self.time.load(Ordering::SeqCst) as f32 / 1000.0
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Seconds {
        self.current()
    }
}

/// Счётчик кадров: время монотонно растёт вместе с числом кадров.
///
/// Камера может не отдавать метки времени, но номер кадра есть всегда.
/// Тогда один кадр — это фиксированный шаг, и разбор становится
/// предсказуемым: ровно столько секунд, сколько кадров прошло.
#[derive(Debug)]
pub struct FrameClock {
    frame: AtomicU64,
    step_millis: AtomicU64,
}

impl FrameClock {
    /// Создаёт часы с шагом по умолчанию в 30 кадров в секунду.
    pub fn new() -> Self {
        Self::with_fps(gesture_core::pipeline::ASSUMED_FPS)
    }

    /// Создаёт часы с заданной частотой кадров.
    pub fn with_fps(fps: f32) -> Self {
        let fps = if fps > 0.0 {
            fps
        } else {
            gesture_core::pipeline::ASSUMED_FPS
        };
        Self {
            frame: AtomicU64::new(0),
            step_millis: AtomicU64::new((1000.0 / fps).round().max(1.0) as u64),
        }
    }

    /// Считает очередной кадр и возвращает его метку времени.
    pub fn tick(&self) -> Seconds {
        let index = self.frame.fetch_add(1, Ordering::SeqCst);
        index as f32 * self.step_millis.load(Ordering::SeqCst) as f32 / 1000.0
    }

    /// Сколько кадров разобрано.
    pub fn frames(&self) -> u64 {
        self.frame.load(Ordering::SeqCst)
    }
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FrameClock {
    fn now(&self) -> Seconds {
        self.tick()
    }
}

/// Готовый источник времени в виде общей ссылки.
pub type SharedClock = Arc<dyn Clock>;

fn to_millis(seconds: Seconds) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds * 1000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_never_goes_backward() {
        let clock = ManualClock::new();
        assert_eq!(clock.advance(0.5), 0.5);
        assert_eq!(clock.advance(0.25), 0.75);
        // Отрицательный шаг игнорируется, а не откатывает время.
        assert_eq!(clock.advance(-10.0), 0.75);
        assert_eq!(clock.current(), 0.75);
    }

    #[test]
    fn manual_clock_can_start_anywhere() {
        let clock = ManualClock::starting_at(10.0);
        assert_eq!(clock.now(), 10.0);
        assert_eq!(clock.advance(1.5), 11.5);
    }

    #[test]
    fn frame_clock_counts_frames_into_seconds() {
        let clock = FrameClock::with_fps(10.0);
        assert_eq!(clock.tick(), 0.0);
        assert_eq!(clock.tick(), 0.1);
        assert_eq!(clock.frames(), 2);
    }

    #[test]
    fn frame_clock_falls_back_on_bogus_fps() {
        let clock = FrameClock::with_fps(0.0);
        assert_eq!(clock.tick(), 0.0);
        assert!(clock.tick() > 0.0);
    }

    #[test]
    fn shared_clock_is_usable_as_trait_object() {
        let clock: SharedClock = Arc::new(ManualClock::starting_at(3.0));
        assert_eq!(clock.now(), 3.0);
    }
}
