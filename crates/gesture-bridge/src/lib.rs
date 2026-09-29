//! Слой между ядром жестов и интерфейсом приложения.
//!
//! Крейт переводит поток кадров на язык интерфейса: принимает
//! [`FrameInput`], отдаёт [`AppState`] и [`BridgeResult`]. Он не зависит ни
//! от Tauri, ни от видеобиблиотек, поэтому команды окна и вся логика
//! проверяются обычными `cargo test`.
//!
//! Разделение намеренное: `gesture-core` решает *что* произошло, мост
//! отвечает *как это показать*, а `src-tauri` только пересылает вызовы.
//!
//! # Пример
//!
//! ```
//! use gesture_bridge::{FrameInput, GestureBridge, ManualClock};
//! use std::sync::Arc;
//!
//! let clock = Arc::new(ManualClock::new());
//! let mut bridge = GestureBridge::with_clock(clock.clone());
//!
//! // Пустой кадр: рук нет, состояние всё равно готово к отрисовке.
//! let result = bridge.ingest(&FrameInput::empty(0.0));
//! assert!(result.event.is_empty());
//! assert!(result.state.signs.is_empty());
//!
//! // Интерфейс переключает режим без участия камеры.
//! let state = bridge.set_mode(gesture_core::AppMode::Translate);
//! assert_eq!(state.mode_title, "Перевод");
//! ```
//!
//! # Отчёт о плохом кадре
//!
//! Кисть с неверным числом точек не роняет кадр, а попадает в
//! [`BridgeResult::rejected`]:
//!
//! ```
//! use gesture_bridge::{FrameInput, GestureBridge, PointSer};
//!
//! let mut bridge = GestureBridge::new();
//! let frame = FrameInput {
//!     time: 0.0,
//!     hands: vec![vec![PointSer { x: 0.0, y: 0.0 }; 7]],
//! };
//!
//! let result = bridge.ingest(&frame);
//! assert_eq!(result.rejected.len(), 1);
//! assert_eq!(result.rejected[0].received, 7);
//! assert_eq!(result.rejected[0].expected, 21);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod bridge;
pub mod clock;
pub mod error;
pub mod state;

pub use bridge::{BridgeMetrics, BridgeResult, GestureBridge, RejectedHand};
pub use clock::{Clock, FrameClock, ManualClock, MonotonicClock, Seconds, SharedClock};
pub use error::BridgeError;
pub use state::{AppState, FrameInput, ScreenRow, SignRow};

/// Точка кисти в сериализуемом виде: она входит в публичный API
/// [`FrameInput`], поэтому переэкспортируется вместе с ним.
pub use gesture_core::pipeline::{EngineEvent, PointSer};

/// Режим работы приложения. Интерфейс переключает его командой, поэтому
/// тип тоже принадлежит контракту моста, а не только ядру.
pub use gesture_core::AppMode;
