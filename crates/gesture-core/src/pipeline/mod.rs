//! Движок приложения: режимы, запись жестов, перевод фраз и очередь команд.
//!
//! Перенос `GestureViewModel` из `GestureViewModel.swift`.
//!
//! Движок не знает ни о камере, ни об окне: на вход подаются точки кистей,
//! на выход — снимок состояния и список событий (озвучивание текста, команды
//! интерфейса). Так его можно полностью проверить тестами и переиспользовать
//! в консольном режиме.
//!
//! Оба режима работают через один автомат `SignRecognizer`: различие в том,
//! что считается результатом, задаёт замыкание-классификатор `classify`.
//! В режиме управления это встроенные жесты, в режиме перевода — ближайший
//! жест из словаря.

mod classify;
mod constants;
mod engine;
mod events;
mod speaker;

#[cfg(test)]
mod tests;

pub use constants::*;
pub use engine::GestureEngine;
pub use events::{EngineEvent, EngineSnapshot, PointSer, RecordingStateSer};
pub use speaker::{SilentSpeaker, Speaker};
