//! Ядро жестового управления: всё, что можно проверить без камеры и окна.
//!
//! Крейт не зависит ни от Tauri, ни от видеобиблиотек, поэтому вся логика
//! покрывается обычными тестами `cargo test`.
//!
//! # Как устроен конвейер
//!
//! ```text
//! кадр камеры
//!    │  gesture-vision: детектор рук → 21 точка
//!    ▼
//! HandGeometry        нормализация, фильтр по размеру ладони
//!    │
//!    ├── HandFeatures  вектор позы  → k-NN по словарю  (статичные жесты)
//!    └── MotionFeatures вектор + смещение → subsequence DTW (динамика)
//!    ▼
//! GestureRecognizer   конечный автомат: появление → движение → удержание
//!    ▼
//! GestureEngine       режимы, фразы, запись жестов, команды
//! ```
//!
//! # Пример
//!
//! ```
//! use gesture_core::{AppMode, GestureEngine};
//!
//! let mut engine = GestureEngine::new();
//! engine.set_mode(AppMode::Control);
//! // Пустой кадр: рук нет, ничего не распознаётся.
//! let (state, event) = engine.process_frame(0.0, &[]);
//! assert!(state.sign.is_none());
//! assert!(event.commands.is_empty());
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod error;
pub mod features;
pub mod geometry;
pub mod joint;
pub mod library;
pub mod models;
pub mod motion;
pub mod pipeline;
pub mod recognizer;

pub use error::{PipelineError, VisionError};
pub use features::HandFeatures;
pub use geometry::{dist, HandGeometry, Point};
pub use library::{CustomSign, JsonFileStore, LibraryError, MemoryStore, SignLibrary, SignStore};
pub use models::{AppMode, Command, DemoScreen, Gesture, RecordingState, Sign, DEMO_SCREENS};
pub use motion::MotionFeatures;
pub use pipeline::{
    EngineEvent, EngineSnapshot, GestureEngine, RecordingStateSer, SilentSpeaker, Speaker,
};
pub use recognizer::{
    BuiltInRecognizer, GestureRecognizer, GestureResult, RecognizerMotion, SignRecognizer,
};
